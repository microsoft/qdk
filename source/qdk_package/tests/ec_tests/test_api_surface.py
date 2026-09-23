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
    "filled",
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
        "derive",
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
    with pytest.raises(AttributeError, match="derive"):
        getattr(ec, "derive")


@pytest.mark.parametrize("module_name", _RETIRED_MODULES)
def test_retired_module_is_not_importable(module_name: str) -> None:
    importlib.invalidate_caches()
    sys.modules.pop(module_name, None)

    assert importlib.util.find_spec(module_name) is None
    with pytest.raises(ModuleNotFoundError, match=module_name):
        importlib.import_module(module_name)


def test_function_signatures() -> None:
    assert (
        str(inspect.signature(ec.filled))
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
        "Location",
        "after",
        "locations",
        "weight",
    }
    assert (
        inspect.signature(ec.FaultEvent.after).parameters["readout_flips"].kind
        is inspect.Parameter.KEYWORD_ONLY
    )
    with pytest.raises(TypeError, match="integers"):
        ec.FaultEvent.after(3, readout_flips=[True])


def test_fault_event_locations_are_ordered_combined_changes() -> None:
    event = (
        ec.FaultEvent.after(7, readout_flips=[2, 0])
        * ec.FaultEvent.after(3, ec.Pauli("X_4"))
        * ec.FaultEvent.after(3, readout_flips=1)
    )
    locations = event.locations
    assert isinstance(locations, tuple)
    assert [location.after_call for location in locations] == [3, 7]
    assert locations[0].error == ec.Pauli("X_4")
    assert locations[0].readout_flips == frozenset({1})
    assert locations[1].error == ec.Pauli.identity()
    assert locations[1].readout_flips == frozenset({0, 2})
    assert all(isinstance(location, ec.FaultEvent.Location) for location in locations)
    assert locations == event.locations
    assert hash(locations) == hash(event.locations)


def test_fault_event_mapping_order_does_not_affect_identity() -> None:
    first = ec.FaultEvent({7: ec.Pauli("X_0"), 2: ec.Pauli("Z_1")})
    second = ec.FaultEvent({2: ec.Pauli("Z_1"), 7: ec.Pauli("X_0")})
    assert [location.after_call for location in first.locations] == [2, 7]
    assert first == second
    assert hash(first) == hash(second)
    assert str(first) == str(second)
    assert repr(first) == repr(second)
    assert first.locations is first.locations


def test_fault_event_product_merges_locations_preserving_pauli_order() -> None:
    left = ec.FaultEvent({9: ec.Pauli("X_0"), 5: ec.Pauli("X_2"), 1: ec.Pauli("Z_3")})
    left *= ec.FaultEvent.after(5, readout_flips=[0, 1])
    right = ec.FaultEvent({8: ec.Pauli("Z_0"), 5: ec.Pauli("Z_2"), 4: ec.Pauli("X_1")})
    right *= ec.FaultEvent.after(5, readout_flips=[1, 2])
    product = left * right
    assert [location.after_call for location in product.locations] == [1, 4, 5, 8, 9]
    assert product.locations[2].error == ec.Pauli("X_2") * ec.Pauli("Z_2")
    assert product.locations[2].readout_flips == frozenset({0, 2})
    assert product != right * left
    assert product.locations[0] is left.locations[0]
    assert product.locations[1] is right.locations[0]
    assert left * ec.FaultEvent() == ec.FaultEvent() * left == left


def test_fault_event_locations_omit_canceled_changes() -> None:
    event = ec.FaultEvent.after(3, ec.Pauli("X_4"), readout_flips=0)
    remaining = event * ec.FaultEvent.after(3, ec.Pauli("X_4"))
    assert len(remaining.locations) == 1
    assert remaining.locations[0].error == ec.Pauli.identity()
    assert remaining.locations[0].readout_flips == frozenset({0})
    assert (event * event).locations == ()
    assert ec.FaultEvent().locations == ()
    assert ec.FaultEvent({3: ec.Pauli.identity()}).locations == ()


@pytest.mark.parametrize("constructor", ["after", "mapping"])
def test_fault_event_copies_input_paulis(constructor: str) -> None:
    error = ec.Pauli("X_4")
    event = (
        ec.FaultEvent.after(3, error)
        if constructor == "after"
        else ec.FaultEvent({3: error})
    )
    expected = ec.FaultEvent.after(3, ec.Pauli("X_4"))
    original_hash = hash(event)
    original_locations = event.locations
    error *= ec.Pauli("Z_4")
    assert event == expected
    assert hash(event) == original_hash
    assert event.locations == original_locations


def test_fault_event_location_errors_are_defensive_copies() -> None:
    event = ec.FaultEvent.after(3, ec.Pauli("X_4"), readout_flips=0)
    (location,) = event.locations
    original_event_hash = hash(event)
    original_location_hash = hash(location)
    error = location.error
    error *= ec.Pauli("Z_4")
    assert location.error == ec.Pauli("X_4")
    assert event.locations == (location,)
    assert hash(event) == original_event_hash
    assert hash(location) == original_location_hash
    assert event * event == ec.FaultEvent()


def test_fault_event_location_contract() -> None:
    assert {name for name in dir(ec.FaultEvent) if not name.startswith("_")} == {
        "Location",
        "after",
        "locations",
        "weight",
    }
    assert {
        name for name in dir(ec.FaultEvent.Location) if not name.startswith("_")
    } == {"after_call", "error", "readout_flips"}
    with pytest.raises(TypeError, match="returned by FaultEvent.locations"):
        ec.FaultEvent.Location()
    (location,) = ec.FaultEvent.after(3, readout_flips=0).locations
    for name, value in (
        ("after_call", 4),
        ("error", ec.Pauli("X_0")),
        ("readout_flips", frozenset()),
    ):
        with pytest.raises((AttributeError, TypeError)):
            setattr(location, name, value)
    assert "after_call=3" in repr(location)


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


@pytest.mark.parametrize(
    "event, expected",
    [
        (ec.FaultEvent(), "no fault"),
        (ec.FaultEvent.after(2, ec.Pauli("X_0")), "X_0 after call 2"),
        (ec.FaultEvent.after(2, ec.Pauli("X_0 Z_3")), "X_0 Z_3 after call 2"),
        (ec.FaultEvent.after(2, ec.Pauli("-Y_2")), "-Y_2 after call 2"),
        (ec.FaultEvent.after(2, readout_flips=0), "flip call 2 readout 0"),
        (
            ec.FaultEvent.after(2, readout_flips=[3, 0]),
            "flip call 2 readouts 0, 3",
        ),
        (
            ec.FaultEvent.after(2, ec.Pauli("X_0"), readout_flips=0),
            "(X_0 after call 2; flip call 2 readout 0)",
        ),
        (
            ec.FaultEvent.after(7, readout_flips=0)
            * ec.FaultEvent.after(2, ec.Pauli("X_0")),
            "(X_0 after call 2; flip call 7 readout 0)",
        ),
        (
            ec.FaultEvent.after(2, ec.Pauli("X_0"))
            * ec.FaultEvent.after(2, ec.Pauli("X_0")),
            "no fault",
        ),
    ],
)
def test_fault_event_str_describes_pauli_errors_and_call_local_readouts(
    event: ec.FaultEvent, expected: str
) -> None:
    assert str(event) == expected
    assert f"{event}" == expected


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


def test_fault_effect_display_uses_the_reference_set() -> None:
    effect = ec.FaultEffect(["checks[3,1,3]", "out[0].z[1]"])
    assert str(effect) == "['checks[1]', 'checks[3]', 'out[0].z[1]']"
    assert repr(effect) == f"FaultEffect({effect})"
    assert eval(repr(effect), {"FaultEffect": ec.FaultEffect}) == effect
    assert str(ec.FaultEffect()) == "[]"


def test_fault_effect_membership_accepts_one_normalized_reference() -> None:
    import qodec as qc

    effect = ec.FaultEffect(["checks[1,3]", "out[0].z[1]"])
    assert "checks[1]" in effect
    assert qc.Reference("checks[01]") in effect
    assert "readouts[0]" not in effect
    assert "out[0].code.z[1]" in effect
    assert "checks[1:2]" in effect
    with pytest.raises(ValueError, match="single target"):
        "checks[1,3]" in effect


def test_fault_effect_normalization_controls_equality_and_hashing() -> None:
    import qodec as qc

    effect = ec.FaultEffect(["checks[3,1,3]", "out[0].z[1]"])
    assert len(effect) == 3
    assert all(isinstance(reference, qc.Reference) for reference in effect)
    assert effect == ec.FaultEffect(["out[0].code.z[01]", "checks[1:4:2]"])
    assert ec.FaultEffect(["checks[1,1]"]) == ec.FaultEffect(["checks[1]"])
    assert effect != frozenset(effect)
    assert effect != ec.FaultEffect()
    assert hash(effect) == hash(ec.FaultEffect(reversed(list(effect))))


def test_fault_effect_xor_cancels_repeated_changes() -> None:
    effect = ec.FaultEffect(["checks[1]", "out[0].z[1]"])
    assert effect ^ effect == ec.FaultEffect()
    assert effect ^ ec.FaultEffect(["checks[1]"]) == ec.FaultEffect(["out[0].z[1]"])
    assert not ec.FaultEffect()
    with pytest.raises(TypeError):
        effect ^ frozenset()


def test_fault_effect_surface_is_opaque_and_immutable() -> None:
    effect = ec.FaultEffect(["checks[1]"])
    assert {name for name in dir(effect) if not name.startswith("_")} == {
        "checks",
        "readouts",
        "frames",
    }
    with pytest.raises(AttributeError):
        effect._references = frozenset()


@pytest.mark.parametrize(
    "reference",
    [
        "in[0].x[0]",
        "circuit.readouts[0]",
        'metadata["name"]',
        "out[0].X_1",
        "out[0].x",
        "out[0:2].x[0]",
        "checks[-1]",
        "checks[0:0]",
        "outputs[0].code.z[1]",
        "",
        "checks",
        "checks[1].equation[0]",
    ],
)
def test_fault_effect_rejects_invalid_targets(reference: str) -> None:
    with pytest.raises(ValueError):
        ec.FaultEffect([reference])
    with pytest.raises(ValueError):
        reference in ec.FaultEffect()


def test_fault_effect_orders_targets_and_copies_input() -> None:
    references = [
        "out[10].x[0]",
        "out[2].stabilizers[0]",
        "checks[10,2]",
        "readouts[1]",
        "out[2].z[1]",
        "out[2].x[3,0]",
    ]
    effect = ec.FaultEffect(iter(references))
    references.clear()
    assert [str(reference) for reference in effect] == [
        "checks[2]",
        "checks[10]",
        "readouts[1]",
        "out[2].x[0]",
        "out[2].x[3]",
        "out[2].z[1]",
        "out[2].stabilizers[0]",
        "out[10].x[0]",
    ]
    assert effect ^ ec.FaultEffect(["checks[2]"]) == ec.FaultEffect(list(effect)[1:])
    for invalid in (1, True, None):
        with pytest.raises(TypeError):
            ec.FaultEffect([invalid])
        with pytest.raises(TypeError):
            invalid in effect
    with pytest.raises(TypeError, match="iterable"):
        ec.FaultEffect("checks[0]")


@pytest.mark.parametrize(
    "references",
    [
        [],
        ["checks[10,2]"],
        ["readouts[3,0]"],
        ["out[1].x[0]", "out[0].z[2]", "out[0].stabilizers[1]"],
        [
            "checks[10,2]",
            "readouts[3,0]",
            "out[1].x[0]",
            "out[0].z[2]",
            "out[0].stabilizers[1]",
        ],
    ],
)
def test_fault_effect_filtered_views_partition_the_effect(
    references: list[str],
) -> None:
    import qodec as qc

    effect = ec.FaultEffect(references)
    original = ec.FaultEffect(references)
    for name, root in (
        ("checks", "checks["),
        ("readouts", "readouts["),
        ("frames", "out["),
    ):
        view = getattr(effect, name)
        expected = tuple(
            reference for reference in effect if reference.path.startswith(root)
        )
        assert isinstance(view, tuple)
        assert all(isinstance(reference, qc.Reference) for reference in view)
        assert view == expected
        assert bool(view) == bool(expected)
        assert hash(view) == hash(expected)
        if view:
            assert qc.Reference(view[0].path) in view
            assert view[0].path not in view
        with pytest.raises(AttributeError):
            getattr(view, "append")(qc.Reference("checks[0]"))
        with pytest.raises((AttributeError, TypeError)):
            setattr(effect, name, ())
    assert effect.checks + effect.readouts + effect.frames == tuple(effect)
    assert ec.FaultEffect(effect.checks + effect.readouts + effect.frames) == effect
    assert effect == original


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
        allowed = ec.FaultEffect(
            [
                *(f"checks[{index}]" for index in range(len(profile.checks))),
                *(f"readouts[{index}]" for index in range(len(profile.readouts))),
                *(
                    f"out[{index}].{basis}[0]"
                    for index in range(len(outputs))
                    for basis in ("x", "z")
                ),
            ]
        )
        assert set(effect) <= set(allowed)
    assert any(
        reference.path.startswith(("checks[", "readouts["))
        for _, effect in profile.fault_effects
        for reference in effect
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


def test_filled_rejects_bare_circuit(idle_gadget) -> None:
    with pytest.raises(TypeError, match="Gadget or qodec.Qodec"):
        ec.filled(idle_gadget.circuit)
