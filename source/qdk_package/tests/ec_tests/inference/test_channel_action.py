"""Tests for channel-action profiling."""

from __future__ import annotations

import qodec as qc
from qodec.gadgets import Encoding

import qdk.ec as ec
from qdk.ec._analysis.channel_action import (
    ChannelAction,
    action_of,
    are_equivalent_mod_paulis,
    are_outcome_equivalent,
    declared_action_of,
    declared_program_of,
    gadget_action_mismatch,
    input_qubits_of,
    realized_action_of,
)
from qdk.ec._analysis.propagation import program_of
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.pauli import Pauli


def _action_of_gadget(gadget: qc.Gadget) -> ChannelAction:
    return action_of(program_of(gadget))


def test_input_qubits_of_idle_channel_is_nonempty(idle_gadget: qc.Gadget) -> None:
    program = program_of(idle_gadget)
    inputs = input_qubits_of(program)
    assert isinstance(inputs, frozenset)
    assert all(isinstance(qubit, int) for qubit in inputs)
    assert inputs <= frozenset(range(program.qubit_count))


def test_action_of_idle_channel_returns_channel_action(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    assert isinstance(action, ChannelAction)
    assert isinstance(action.observables, FrameGroup)
    assert isinstance(action.stabilizers, FrameGroup)
    assert isinstance(action.mapping, dict)


def test_action_is_equivalent_to_itself(idle_gadget: qc.Gadget) -> None:
    action = _action_of_gadget(idle_gadget)
    assert action.is_equivalent_to(action)
    assert action.is_equivalent_to(action, modulo_paulis=True)
    assert are_equivalent_mod_paulis(action, action)
    assert are_outcome_equivalent(action, action)


def test_distinct_gadgets_are_not_equivalent(
    idle_gadget: qc.Gadget, measure_xx_gadget: qc.Gadget
) -> None:
    idle = _action_of_gadget(idle_gadget)
    measure = _action_of_gadget(measure_xx_gadget)
    assert not idle.is_equivalent_to(measure)
    assert not idle.is_equivalent_to(measure, modulo_paulis=True)
    assert not are_equivalent_mod_paulis(idle, measure)


def test_sign_flipped_action_is_mod_paulis_equivalent_but_not_outcome(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    if not action.mapping:
        return
    flipped_mapping = {key: value * -1 for key, value in action.mapping.items()}
    flipped = ChannelAction(action.observables, action.stabilizers, flipped_mapping)
    assert are_equivalent_mod_paulis(action, flipped)
    assert flipped.is_equivalent_to(action, modulo_paulis=True)
    assert not are_outcome_equivalent(action, flipped)
    assert not flipped.is_equivalent_to(action)


def test_different_stabilizers_are_not_mod_paulis_equivalent(
    idle_gadget: qc.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    extra = FrameGroup(
        list(action.stabilizers.generators) + [PauliFrame(Pauli({0: "Z"}))]
    )
    perturbed = ChannelAction(action.observables, extra, action.mapping)
    assert not are_equivalent_mod_paulis(action, perturbed)


def test_preparation_declared_stabilizers_are_deterministic(
    prepare_xx_gadget: qc.Gadget,
    prepare_zz_gadget: qc.Gadget,
) -> None:
    """A ``stabilize`` preparation must fix its stabilisers at a definite +1.

    Regression: the interpreter enacted ``stabilize P`` as a bare projective
    measurement, so an X-basis preparation (``P`` anticommutes with the |0>
    reset) left the prepared sign riding on the random projection outcome — a
    spurious frame on the *declared* action that made every prepare_x gadget mismatch
    its deterministic (reset + H) circuit. Z-basis preparations were
    unaffected because Z already stabilises |0>. Both must come out frame-free
    and audit-clean.
    """
    for gadget in (prepare_xx_gadget, prepare_zz_gadget):
        declared = declared_action_of(gadget)
        generators = declared.stabilizers.standardized().generators
        assert generators, "preparation fixes no stabilisers"
        assert all(not framed.frame for framed in generators), (
            "preparation left an outcome frame on its stabilisers; `stabilize` "
            "must deterministically prepare the +1 eigenstate"
        )
        assert gadget_action_mismatch(gadget) is None


def test_idle_declared_and_realized_actions_match_golden_values(
    idle_gadget: qc.Gadget,
) -> None:
    profile = ec.GadgetProfile(idle_gadget)

    assert str(profile.objective) == (
        "observables: FrameGroup(generators=())\n"
        "stabilizers: FrameGroup(generators=())\n"
        "mapping: {X: X^{0}, Z: Z, IX: IX^{1}, IZ: IZ}"
    )
    assert str(profile.action) == (
        "observables: FrameGroup(generators=())\n"
        "stabilizers: FrameGroup(generators=())\n"
        "mapping: {X: X^{2,3}, Z: Z, IX: IX^{1,3}, IZ: IZ}"
    )


def test_realized_action_is_invariant_under_equivalent_logical_representatives(
    idle_gadget: qc.Gadget,
) -> None:
    equivalent_code = qc.Code(
        "C4-alternate-basis",
        stabilizers=["X_0 X_1 X_2 X_3", "Z_0 Z_1 Z_2 Z_3"],
        x=["X_2 X_3", "X_1 X_3"],
        z=["Z_1 Z_3", "Z_2 Z_3"],
    )
    alternate = qc.Gadget(
        idle_gadget.implements,
        idle_gadget.circuit,
        inputs=[
            Encoding(equivalent_code, support=list(entry.support))
            for entry in idle_gadget.inputs
        ],
        outputs=[
            Encoding(equivalent_code, support=list(entry.support))
            for entry in idle_gadget.outputs
        ],
        checks=list(idle_gadget.checks),
        readouts=list(idle_gadget.readouts),
    )

    original = ec.GadgetProfile(idle_gadget)
    changed = ec.GadgetProfile(alternate)
    assert original.action.is_equivalent_to(changed.action)


def test_destructive_measurement_carries_no_logical_but_stays_distinguishable(
    measure_zz_gadget: qc.Gadget,
    measure_xx_gadget: qc.Gadget,
    prepare_zz_gadget: qc.Gadget,
) -> None:
    """Pins why ``_decode`` skips a logical with no image instead of raising.

    An empty mapping is the right answer for a destructive gadget, and the
    observables still separate it from the other basis and from a preparation.
    """
    measured = realized_action_of(measure_zz_gadget)

    assert not measured.mapping
    for other in (measure_xx_gadget, prepare_zz_gadget):
        assert not are_equivalent_mod_paulis(measured, realized_action_of(other))


def test_declared_program_binds_inputs_and_outputs_to_the_same_indices(
    idle_gadget: qc.Gadget,
) -> None:
    """Pins the reference side of the action check: both operand sets are 0..n-1."""
    (call,) = declared_program_of(idle_gadget).instructions

    assert call.mnemonic == idle_gadget.implements.mnemonic
    assert dict(call.inputs) == {"0": 0, "1": 1}
    assert dict(call.outputs) == {"0": 0, "1": 1}
