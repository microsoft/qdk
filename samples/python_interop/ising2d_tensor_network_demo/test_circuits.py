# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Frozen χ-sweep circuits and CPU references (pinned reference environment)."""

import math
import shutil

import numpy as np
import pytest

from build_measured_circuit import build_measured_qir
from circuits import (
    CASE_A_ORACLE,
    RECIPE,
    case_a_reference,
    gates_from_measured,
    generate,
    verify_circuits,
)
from ising import (
    CASE_A,
    CIRCUITS,
    bonds,
    circuit_name,
    circuit_path,
    correlation_terms,
    cpu_reference,
    frozen_cases,
    magnetization_terms,
    z_expectation,
)
from reference import check_case_a_schedule


def test_frozen_circuits_and_references_replay():
    report = verify_circuits()
    assert report["passed"]
    assert report["4x4_h3.03"]["m_z"] == pytest.approx(0.4937060, abs=5e-8)
    assert report["4x4_h3.03"]["c_zz"] == pytest.approx(0.5523722, abs=5e-8)


def test_every_catalogued_case_has_a_frozen_circuit():
    for size, field in frozen_cases():
        path = circuit_path(size, field)
        assert path.is_file()
        assert f'"required_num_qubits"="{size * size}"' in path.read_text()


@pytest.mark.parametrize("name", [circuit_name(5, 3.03), "cpu_reference.json"])
def test_changed_fixture_bytes_are_not_accepted(tmp_path, name):
    copy = tmp_path / "circuits"
    shutil.copytree(CIRCUITS, copy)
    data = bytearray((copy / name).read_bytes())
    data[len(data) // 2] ^= 1
    (copy / name).write_bytes(bytes(data))
    with pytest.raises(ValueError, match="hash mismatch"):
        verify_circuits(copy)


def test_default_field_reproduces_the_frozen_case_a_circuit():
    assert build_measured_qir(4, 4, **RECIPE) == (CASE_A / "measured.ll").read_text()
    assert build_measured_qir(4, 4, h=0.5, **RECIPE) == (CASE_A / "measured.ll").read_text()


def test_schedule_check_distinguishes_the_field():
    parsed = gates_from_measured(circuit_path(4, 3.03).read_text())
    assert check_case_a_schedule(parsed, 4, 4, h=3.03)["maximum_coefficient_error"] == 0.0
    with pytest.raises(ValueError, match="Suzuki coefficients"):
        check_case_a_schedule(parsed, 4, 4)


def test_case_a_amplitudes_reproduce_the_published_oracle():
    values = case_a_reference()
    assert {name: round(values[name], 7) for name in CASE_A_ORACLE} == CASE_A_ORACLE
    assert cpu_reference(4, 0.5)["m_z"] == values["m_z"]


@pytest.mark.parametrize("theta", [0.0, 0.3, -1.1, math.pi])
def test_observables_on_a_product_state_of_rx_rotations(theta):
    # Rx(θ)|0> on every site of a 2×2 lattice: <Z_q> = cos θ, <Z_i Z_j> = cos² θ.
    single = np.array([math.cos(theta / 2) ** 2, math.sin(theta / 2) ** 2])
    distribution = single
    for _ in range(3):
        distribution = np.kron(single, distribution)
    assert z_expectation(distribution, magnetization_terms(2)) == pytest.approx(math.cos(theta))
    assert z_expectation(distribution, correlation_terms(2)) == pytest.approx(math.cos(theta) ** 2)


def test_observables_are_little_endian_and_use_every_bond():
    assert bonds(3) == [(0, 1), (1, 2), (3, 4), (4, 5), (6, 7), (7, 8), (0, 3), (1, 4), (2, 5), (3, 6), (4, 7), (5, 8)]
    assert all(len(bonds(n)) == 2 * n * (n - 1) for n in (2, 4, 10))
    basis = np.zeros(4)
    basis[0b01] = 1.0  # qubit 0 is |1>, qubit 1 is |0>
    assert z_expectation(basis, [("Z", [0], 1.0)]) == -1.0
    assert z_expectation(basis, [("Z", [1], 1.0)]) == 1.0
    assert z_expectation(basis, [("ZZ", [0, 1], 1.0)]) == -1.0
    with pytest.raises(ValueError, match="Only Z"):
        z_expectation(basis, [("X", [0], 1.0)])
    with pytest.raises(ValueError, match="must be real"):
        z_expectation(basis, [("Z", [0], 1j)])


def test_catalogue_rejects_sizes_and_fields_without_a_frozen_circuit():
    for size, field in [(7, 0.5), (4, 1.0)]:
        with pytest.raises(ValueError, match="No frozen circuit"):
            circuit_path(size, field)


def test_generation_refuses_to_overwrite_existing_artifacts(tmp_path):
    sentinel = tmp_path / "sentinel"
    sentinel.write_text("keep")
    with pytest.raises(FileExistsError, match="Refusing to overwrite"):
        generate(tmp_path)
    assert sentinel.read_text() == "keep"
