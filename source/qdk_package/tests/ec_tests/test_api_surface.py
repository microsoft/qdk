"""The deliberately small, flat public surface of ``qdk.ec``."""

from __future__ import annotations

import importlib.util
import inspect
import sys

import pytest

import qdk.ec as ec

_SURFACE = {
    "ChannelAction",
    "Diagnostic",
    "FaultEffect",
    "FaultEvent",
    "GadgetProfile",
    "Pauli",
    "Report",
    "SubsystemCode",
    "audit",
    "build_qodec",
    "derive",
}

_RETIRED_MODULES = (
    "qdk.ec.action",
    "qdk.ec.checks",
    "qdk.ec.code",
    "qdk.ec.distance",
    "qdk.ec.equivalence",
    "qdk.ec.faults",
    "qdk.ec.readouts",
    "qdk.ec.lint",
)


def test_api_surface_is_exact() -> None:
    assert set(ec.__all__) == _SURFACE
    assert set(dir(ec)) == _SURFACE
    assert all(getattr(ec, name) is not None for name in _SURFACE)


def test_old_names_are_not_exported() -> None:
    assert not {
        "action",
        "checks",
        "code",
        "distance",
        "equivalence",
        "faults",
        "lint",
        "readouts",
        "complete_gadget",
        "complete_qodec",
        "qodec_from_code",
    } & set(ec.__all__)


@pytest.mark.parametrize("module_name", _RETIRED_MODULES)
def test_retired_module_is_not_importable(module_name: str) -> None:
    importlib.invalidate_caches()
    sys.modules.pop(module_name, None)

    assert importlib.util.find_spec(module_name) is None
    with pytest.raises(ModuleNotFoundError, match=module_name):
        importlib.import_module(module_name)


def test_function_signatures() -> None:
    assert (
        str(inspect.signature(ec.derive))
        == "(target: 'qc.Gadget | qc.Qodec') -> 'qc.Gadget | qc.Qodec'"
    )
    assert str(inspect.signature(ec.audit)) == (
        "(qodec: 'qc.Qodec', *, disabled: 'Collection[str]' = (), "
        "promote_warnings: 'bool' = False) -> 'Report'"
    )
    assert str(inspect.signature(ec.build_qodec)) == (
        "(code: 'qc.Code | SubsystemCode', *, name: 'str | None' = None, "
        "description: 'str | None' = None, strategy: 'str' = "
        "'flagged-css/v1', strict: 'bool' = True) -> 'qc.Qodec'"
    )


def test_diagnostic_severity_is_nested() -> None:
    diagnostic = ec.Diagnostic(
        "rule", ec.Diagnostic.Severity.WARNING, "summary", "artifact"
    )
    assert diagnostic.severity is ec.Diagnostic.Severity.WARNING
    assert "Severity" not in ec.__all__


def test_fault_event_composition_and_weight() -> None:
    x = ec.Pauli({2: "X"})
    z = ec.Pauli({3: "Z"})
    fault = ec.FaultEvent.after(4, x) * ec.FaultEvent.after(6, z)

    assert fault.weight == 2
    assert fault.locations == {4: x, 6: z}
    assert fault * fault == ec.FaultEvent({})
    assert hash(fault)


def test_subsystem_code_view_is_idempotent(bundle) -> None:
    code = next(iter(bundle.codes.values()))
    view = ec.SubsystemCode.of(code)

    assert ec.SubsystemCode.of(view) is view
    assert isinstance(view.syndrome_of(ec.Pauli.identity()), frozenset)
    assert view.logical_effect_of(ec.Pauli.identity()) == ec.Pauli.identity()
    assert view.why_not_equivalent_to(view) == ""


def test_gadget_profile_contract(idle_gadget) -> None:
    profile = ec.GadgetProfile(idle_gadget)

    assert isinstance(profile.action, ec.ChannelAction)
    assert isinstance(profile.objective, ec.ChannelAction)
    assert all(isinstance(check, frozenset) for check in profile.checks)
    assert all(isinstance(readout, frozenset) for readout in profile.readouts)
    assert profile.why_not_equivalent_to(profile) == ""
    fault, effect = profile.fault_effects[0]
    assert isinstance(fault, ec.FaultEvent)
    assert isinstance(effect, ec.FaultEffect)
    assert profile.effects_of([fault]) == (effect,)


def test_gadget_profile_accepts_a_bare_circuit(idle_gadget) -> None:
    """A circuit is a gadget with trivial encodings, so nothing is silently empty."""
    profile = ec.GadgetProfile(idle_gadget.circuit)

    assert profile.objective is None
    assert isinstance(profile.action, ec.ChannelAction)
    assert all(isinstance(readout, frozenset) for readout in profile.readouts)
    assert all(isinstance(check, frozenset) for check in profile.checks)
    outputs = profile._circuit_outputs
    for _, effect in profile.fault_effects:
        assert set(effect.output_error) == set(range(len(outputs)))
        assert all(position < len(profile.checks) for position in effect.syndrome)
        assert all(
            position < len(profile.readouts) for position in effect.readout_flips
        )
    assert any(
        effect.syndrome or effect.readout_flips for _, effect in profile.fault_effects
    )


def test_gadget_profile_rejects_other_targets() -> None:
    with pytest.raises(TypeError, match="Gadget or qodec.gadgets.Circuit"):
        ec.GadgetProfile(object())


def test_derive_rejects_bare_circuit(idle_gadget) -> None:
    with pytest.raises(TypeError, match="Gadget or qodec.Qodec"):
        ec.derive(idle_gadget.circuit)
