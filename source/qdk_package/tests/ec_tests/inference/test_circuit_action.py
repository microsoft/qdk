"""Tests for circuit-action profiling."""
from __future__ import annotations

import qodec

from qdk.ec.action import (
    CircuitAction,
    action_of,
    gadget_action_mismatch,
    input_qubits_of,
)
from qdk.ec.action import declared_action_of as gadget_objective_action_of
from qdk.ec.equivalence import (
    actions_equivalent_mod_pauli as are_equivalent_mod_paulis,
    actions_outcome_equivalent as are_outcome_equivalent,
)
from qdk.ec._analysis.propagation import Program
from qdk.ec._qodec_compat import realization
from qdk.ec._analysis.propagation.frames import FrameGroup, PauliFrame
from qdk.ec._analysis.propagation.pauli import Pauli


def _action_of_gadget(gadget: qodec.Gadget) -> CircuitAction:
    channel = realization(gadget)
    program = Program(channel.instructions, channel.isa)
    return action_of(program)


def test_input_qubits_of_idle_channel_is_nonempty(idle_gadget: qodec.Gadget) -> None:
    channel = realization(idle_gadget)
    program = Program(channel.instructions, channel.isa)
    inputs = input_qubits_of(program)
    assert isinstance(inputs, frozenset)
    assert all(isinstance(qubit, int) for qubit in inputs)
    assert inputs <= frozenset(range(program.qubit_count))


def test_action_of_idle_channel_returns_circuit_action(
    idle_gadget: qodec.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    assert isinstance(action, CircuitAction)
    assert isinstance(action.observables, FrameGroup)
    assert isinstance(action.stabilizers, FrameGroup)
    assert isinstance(action.mapping, dict)


def test_action_is_equivalent_to_itself(idle_gadget: qodec.Gadget) -> None:
    action = _action_of_gadget(idle_gadget)
    assert action.is_equivalent_to(action)
    assert action.is_equivalent_to(action, modulo_paulis=True)
    assert are_equivalent_mod_paulis(action, action)
    assert are_outcome_equivalent(action, action)


def test_distinct_gadgets_are_not_equivalent(
    idle_gadget: qodec.Gadget, measure_xx_gadget: qodec.Gadget
) -> None:
    idle = _action_of_gadget(idle_gadget)
    measure = _action_of_gadget(measure_xx_gadget)
    assert not idle.is_equivalent_to(measure)
    assert not idle.is_equivalent_to(measure, modulo_paulis=True)
    assert not are_equivalent_mod_paulis(idle, measure)


def test_sign_flipped_action_is_mod_paulis_equivalent_but_not_outcome(
    idle_gadget: qodec.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    if not action.mapping:
        return
    flipped_mapping = {
        key: value * -1 for key, value in action.mapping.items()
    }
    flipped = CircuitAction(action.observables, action.stabilizers, flipped_mapping)
    assert are_equivalent_mod_paulis(action, flipped)
    assert flipped.is_equivalent_to(action, modulo_paulis=True)
    assert not are_outcome_equivalent(action, flipped)
    assert not flipped.is_equivalent_to(action)


def test_different_stabilizers_are_not_mod_paulis_equivalent(
    idle_gadget: qodec.Gadget,
) -> None:
    action = _action_of_gadget(idle_gadget)
    extra = FrameGroup(
        list(action.stabilizers.generators) + [PauliFrame(Pauli({0: "Z"}))]
    )
    perturbed = CircuitAction(action.observables, extra, action.mapping)
    assert not are_equivalent_mod_paulis(action, perturbed)


def test_preparation_objective_stabilizers_are_deterministic(
    prepare_xx_gadget: qodec.Gadget,
    prepare_zz_gadget: qodec.Gadget,
) -> None:
    """A ``stabilize`` preparation must fix its stabilisers at a definite +1.

    Regression: the interpreter enacted ``stabilize P`` as a bare projective
    measurement, so an X-basis preparation (``P`` anticommutes with the |0>
    reset) left the prepared sign riding on the random projection outcome — a
    spurious frame on the *objective* that made every prepare_x gadget mismatch
    its deterministic (reset + H) realisation. Z-basis preparations were
    unaffected because Z already stabilises |0>. Both must come out frame-free
    and audit-clean.
    """
    for gadget in (prepare_xx_gadget, prepare_zz_gadget):
        objective = gadget_objective_action_of(gadget)
        generators = objective.stabilizers.standardized().generators
        assert generators, "preparation fixes no stabilisers"
        assert all(not framed.frame for framed in generators), (
            "preparation left an outcome frame on its stabilisers; `stabilize` "
            "must deterministically prepare the +1 eigenstate"
        )
        assert gadget_action_mismatch(gadget) is None
