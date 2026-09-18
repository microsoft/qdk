"""The deliberately small, flat public surface of ``qdk.ec``."""

from __future__ import annotations

import importlib.util
import inspect
import sys

import pytest

import qdk.ec as ec

_SURFACE = {
    "ChannelAction",
    "CodeProfile",
    "Diagnostic",
    "Distance",
    "FaultEffect",
    "FaultEvent",
    "GadgetProfile",
    "Pauli",
    "Report",
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
        "SubsystemCode",
    } & set(ec.__all__)
    with pytest.raises(AttributeError, match="SubsystemCode"):
        getattr(ec, "SubsystemCode")


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
        "(code: 'qc.Code | CodeProfile', *, name: 'str | None' = None, "
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
    assert fault == ec.FaultEvent({4: x, 6: z})
    assert fault * fault == ec.FaultEvent({})
    assert hash(fault)


def test_readout_faults_are_immutable_and_compose_by_parity() -> None:
    flips = [0, 2]
    readout_fault = ec.FaultEvent.after(3, readout_flips=flips)
    flips.clear()
    assert readout_fault == ec.FaultEvent.after(3, readout_flips=[0, 2])
    assert readout_fault.weight == 2
    assert readout_fault * readout_fault == ec.FaultEvent()
    assert hash(readout_fault) == hash(ec.FaultEvent.after(3, readout_flips=[2, 0]))
    combined = readout_fault * ec.FaultEvent.after(3, ec.Pauli("X_0"))
    assert combined == ec.FaultEvent.after(3, ec.Pauli("X_0"), readout_flips=[0, 2])
    assert combined.weight == 3
    assert combined * ec.FaultEvent.after(3, readout_flips=2) == ec.FaultEvent.after(
        3, ec.Pauli("X_0"), readout_flips=0
    )
    assert {name for name in dir(readout_fault) if not name.startswith("_")} == {
        "after",
        "weight",
    }
    assert (
        inspect.signature(ec.FaultEvent.after).parameters["readout_flips"].kind
        is inspect.Parameter.KEYWORD_ONLY
    )
    with pytest.raises(TypeError, match="integers"):
        ec.FaultEvent.after(3, readout_flips=[True])


def test_fault_event_after_accepts_single_or_multiple_local_readouts() -> None:
    single = ec.FaultEvent.after(7, readout_flips=0)
    assert single == ec.FaultEvent.after(7, readout_flips=[0])
    assert single.weight == 1
    assert single * single == ec.FaultEvent()
    assert single != ec.FaultEvent.after(8, readout_flips=0)
    combined = ec.FaultEvent.after(7, ec.Pauli("X_0"), readout_flips=[0, 2])
    assert combined == (
        ec.FaultEvent.after(7, ec.Pauli("X_0"))
        * single
        * ec.FaultEvent.after(7, readout_flips=2)
    )
    for invalid in (True, False, [True], [0, False]):
        with pytest.raises(TypeError, match="integers"):
            ec.FaultEvent.after(7, readout_flips=invalid)


def test_fault_event_repr_is_replayable() -> None:
    quantum = ec.FaultEvent.after(2, ec.Pauli("X_0"))
    readout = ec.FaultEvent.after(7, readout_flips=0)
    mixed = ec.FaultEvent.after(7, ec.Pauli("Z_0"), readout_flips=[2, 0])
    assert repr(readout) == "FaultEvent.after(7, readout_flips=0)"
    assert repr(mixed) == "FaultEvent.after(7, Pauli('Z'), readout_flips=[0, 2])"
    assert (
        repr(readout * quantum)
        == "FaultEvent.after(2, Pauli('X')) * FaultEvent.after(7, readout_flips=0)"
    )
    for event in (ec.FaultEvent(), quantum, readout, mixed, mixed * quantum):
        assert (
            eval(repr(event), {"FaultEvent": ec.FaultEvent, "Pauli": ec.Pauli}) == event
        )


def test_code_profile_contract() -> None:
    import qodec as qc

    code = qc.Code("repetition_2", ["Z_0 Z_1"], ["X_0 X_1"], ["Z_0"])
    view = ec.CodeProfile(code)

    assert str(inspect.signature(ec.CodeProfile)) == "(code: 'qc.Code') -> 'None'"
    assert {name for name in dir(ec.CodeProfile) if not name.startswith("_")} == {
        "stabilizer",
        "stabilizers",
        "anti_stabilizer",
        "anti_stabilizers",
        "gauge",
        "gauge_basis",
        "logical",
        "logical_basis",
        "support",
        "length",
        "logical_qubit_count",
        "syndrome_of",
        "logical_effect_of",
        "distance",
        "distance_bounds",
        "encoding_clifford",
        "is_trivial_error",
        "is_trivial_logical_error",
        "is_logical_error",
        "is_non_trivial_logical_error",
        "logical_action_of",
        "representative_of",
        "unsigned_logical_action_of",
        "is_equivalent_to",
        "why_not_equivalent_to",
    }
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
    assert {name for name in dir(ec.GadgetProfile) if not name.startswith("_")} == {
        "action",
        "objective",
        "checks",
        "readouts",
        "fault_effects",
        "effects_of",
        "distance",
        "distance_bounds",
        "is_equivalent_to",
        "why_not_equivalent_to",
    }


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


def test_profile_distance_signatures() -> None:
    assert str(inspect.signature(ec.GadgetProfile.distance)) == (
        "(self, *, faults: 'Sequence[FaultEvent] | None' = None, "
        "upper_bound: 'int | None' = None, solver: 'ExactSolver | None' = None) "
        "-> 'Distance[FaultEvent]'"
    )
    assert str(inspect.signature(ec.GadgetProfile.distance_bounds)) == (
        "(self, *, faults: 'Sequence[FaultEvent] | None' = None, "
        "upper_bound: 'int | None' = None, solver: 'BoundsSolver | None' = None) "
        "-> 'Distance[FaultEvent]'"
    )


def test_channel_action_is_opaque(idle_gadget) -> None:
    action = ec.GadgetProfile(idle_gadget).action
    assert {name for name in dir(action) if not name.startswith("_")} == {
        "is_equivalent_to",
        "why_not_equivalent_to",
    }
    assert not inspect.signature(ec.ChannelAction).parameters
    for name in ("observables", "stabilizers", "mapping"):
        assert not hasattr(action, name)
        with pytest.raises(AttributeError):
            setattr(action, name, None)
    with pytest.raises(TypeError, match="GadgetProfile"):
        ec.ChannelAction()
    assert action.is_equivalent_to(action)
    assert action.why_not_equivalent_to(action) == ""


def test_gadget_profile_rejects_other_targets() -> None:
    with pytest.raises(TypeError, match="Gadget or qodec.gadgets.Circuit"):
        ec.GadgetProfile(object())


def test_derive_rejects_bare_circuit(idle_gadget) -> None:
    with pytest.raises(TypeError, match="Gadget or qodec.Qodec"):
        ec.derive(idle_gadget.circuit)
