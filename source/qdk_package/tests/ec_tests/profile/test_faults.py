"""Tests for intrinsic fault profiling."""

import qodec as qc
from qodec.circuits import Program

from qdk.ec.faults import Fault, FaultEffect, FaultProfile, fault_profile_of
from qdk.ec.targets import depolarizing


def _program_of(gadget: qc.Gadget) -> Program:
    return Program(gadget.circuit.instructions, gadget.circuit.isa)


def _basis_of(gadget: qc.Gadget) -> tuple[Fault, ...]:
    return depolarizing(0.001).fault_basis_of(_program_of(gadget))


def test_depolarizing_target_admits_three_faults_per_qubit_per_instruction(
    idle_gadget: qc.Gadget,
) -> None:
    program = _program_of(idle_gadget)
    basis = depolarizing(0.001).fault_basis_of(program)
    expected = 3 * sum(len(call.inputs) for call in program.instructions)
    assert len(basis) == expected


def test_fault_profile_maps_each_basis_element_to_an_intrinsic_effect(
    idle_gadget: qc.Gadget,
) -> None:
    basis = _basis_of(idle_gadget)
    profile = fault_profile_of(idle_gadget, basis)
    assert isinstance(profile, FaultProfile)
    assert profile.basis == basis
    assert len(profile.effects) == len(basis)
    assert all(isinstance(effect, FaultEffect) for effect in profile.effects)
    assert all(not hasattr(effect, "probability") for effect in profile.effects)


def test_fault_profile_of_idle_channel_has_some_detectable_faults(
    idle_gadget: qc.Gadget,
) -> None:
    profile = fault_profile_of(idle_gadget, _basis_of(idle_gadget))
    assert any(effect.flipped_checks for effect in profile.effects)


def test_fault_profile_of_returns_empty_for_empty_basis(
    idle_gadget: qc.Gadget,
) -> None:
    assert fault_profile_of(idle_gadget, ()) == FaultProfile((), ())
