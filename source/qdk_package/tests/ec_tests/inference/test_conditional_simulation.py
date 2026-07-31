"""Tests for the simulator-to-frame-group snapshot.

These exercise :func:`qdk.ec.profile.propagation.frame_group_of`
without committing to paulimer's specific choice of stabiliser representation
(which depends on internal basis choices). What we can pin down:

* The number of generators equals ``simulation.qubit_count``.
* Each generator's Pauli structure equals ``clifford.image_z(q)``.
* Frame ``q`` is the support of ``sign_matrix`` row ``q``.
* Bell-correlation invariants survive a round-trip through the snapshot.
"""
from __future__ import annotations

from paulimer import OutcomeCompleteSimulation, SparsePauli, UnitaryOpcode

from qdk.ec.profile.propagation import frame_group_of
from qdk.ec.profile.propagation.frames import FrameGroup
from qdk.ec.profile.propagation.pauli import Pauli


def _fresh(qubit_count: int) -> OutcomeCompleteSimulation:
    sim = OutcomeCompleteSimulation.with_capacity(qubit_count, 32, 32)
    sim.reserve_qubits(qubit_count)
    sim.reserve_outcomes(32, 32)
    return sim


def _expected_frame(
    simulation: OutcomeCompleteSimulation, qubit: int
) -> frozenset[int]:
    return frozenset(list(simulation.sign_matrix.rows)[qubit].support)


# ── Basic invariants ────────────────────────────────────────────────────────


def test_fresh_simulator_yields_empty_frames() -> None:
    sim = _fresh(3)
    group = frame_group_of(sim)

    assert isinstance(group, FrameGroup)
    assert len(group.generators) == sim.qubit_count == 3
    for entry in group.generators:
        assert entry.frame == frozenset()


def test_pauli_structures_match_clifford_image_z() -> None:
    sim = _fresh(2)
    sim.apply_unitary(UnitaryOpcode.Hadamard, [0])
    sim.apply_unitary(UnitaryOpcode.ControlledX, [0, 1])

    group = frame_group_of(sim)
    clifford = sim.clifford
    for qubit, entry in enumerate(group.generators):
        assert entry.pauli == Pauli.from_dense(clifford.image_z(qubit))


# ── After measurements ─────────────────────────────────────────────────────


def test_frames_match_sign_matrix_after_measurements() -> None:
    sim = _fresh(2)
    # Put each qubit in a superposition then measure Z (each gives a random bit).
    sim.apply_unitary(UnitaryOpcode.Hadamard, [0])
    sim.apply_unitary(UnitaryOpcode.Hadamard, [1])
    sim.measure(SparsePauli({0: "Z"}))
    sim.measure(SparsePauli({1: "Z"}))

    group = frame_group_of(sim)
    assert sim.sign_matrix.column_count >= 2  # two random bits introduced
    for qubit, entry in enumerate(group.generators):
        assert entry.frame == _expected_frame(sim, qubit)


# ── Bell correlations ──────────────────────────────────────────────────────


def test_bell_then_data_z_measurement_correlates_aux_z_with_data_z() -> None:
    """Bell-pair (0=data, 1=aux), measure Z on data — Z_0 and Z_1 should
    factor to the same outcome frame because Z_0 Z_1 is a stabiliser with
    sign +1 (the Bell-Z) so Z_0 ≡ Z_1 modulo it.
    """
    sim = _fresh(2)
    sim.apply_unitary(UnitaryOpcode.PrepareBell, [0, 1])
    sim.measure(SparsePauli({0: "Z"}))

    group = frame_group_of(sim)
    z_0 = Pauli({0: "Z"})
    z_1 = Pauli({1: "Z"})
    assert group.factorization_of(z_0) is not None
    assert group.factorization_of(z_1) is not None
    assert group.frame_of(z_0) == group.frame_of(z_1)


def test_frame_of_xors_factor_frames_consistently() -> None:
    sim = _fresh(2)
    sim.apply_unitary(UnitaryOpcode.Hadamard, [0])
    sim.apply_unitary(UnitaryOpcode.Hadamard, [1])
    sim.measure(SparsePauli({0: "Z"}))
    sim.measure(SparsePauli({1: "Z"}))

    group = frame_group_of(sim)
    z_0 = Pauli({0: "Z"})
    z_1 = Pauli({1: "Z"})
    factors = group.factorization_of(z_0 * z_1)
    assert factors is not None
    accumulated: frozenset[int] = frozenset()
    for factor in factors:
        accumulated ^= factor.frame
    assert group.frame_of(z_0 * z_1) == accumulated


def test_deterministic_measurement_does_not_widen_frames() -> None:
    """Measuring an observable that is already a stabiliser is deterministic;
    it should not add a random column to the sign matrix, so frames stay
    empty."""
    sim = _fresh(1)
    # Z_0 is already a stabiliser of |0⟩, so measuring Z_0 is deterministic.
    width_before = sim.sign_matrix.column_count
    sim.measure(SparsePauli({0: "Z"}))
    assert sim.sign_matrix.column_count == width_before

    group = frame_group_of(sim)
    for entry in group.generators:
        assert entry.frame == frozenset()
