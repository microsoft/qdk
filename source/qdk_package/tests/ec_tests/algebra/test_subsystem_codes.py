from typing import Sequence
from itertools import zip_longest, product
import pytest
from more_itertools import interleave, chunked
from paulimer import SparsePauli as RustSparsePauli
from qdk.ec.profile.code_algebra import (
    encoding_clifford_of,
    SubsystemCode,
    clifford_images_of,
    _validate_anti_stabilizers,
)
from ec_tests.testing import code_catalog
from paulimer import PauliGroup

from qdk.ec.profile.propagation.pauli import Pauli, PauliEnumerator, identity


bacon_shor_codes = [
    code_catalog.make_bacon_shor_code(number_of_rows, number_of_columns)
    for number_of_rows in range(2, 6)
    for number_of_columns in range(2, 6)
]
subsystem_codes = bacon_shor_codes


@pytest.mark.parametrize("code", subsystem_codes)
def test_consistency_of(code: SubsystemCode) -> None:
    assert_consistency_of(code)


@pytest.mark.parametrize("code", subsystem_codes)
def test_encoding_clifford_of(code: SubsystemCode) -> None:
    assert_encoding_clifford_of(code)


def assert_consistency_of(code: SubsystemCode) -> None:
    assert_code_generators(code)
    assert_valid_logical_basis(code)
    assert_valid_logical_actions(code)
    assert_valid_representatives(code)
    assert_anti_generators(code)
    assert_group_property_consistency_of(code)
    assert_subsystem_init_consistency_of(code)


def assert_subsystem_init_consistency_of(code: SubsystemCode) -> None:
    def assert_clone(
        gauge_basis: Sequence[Pauli] | None = None,
        # anti_stabilizers: Sequence[Pauli] | None = None,
    ) -> None:
        clone = SubsystemCode(
            code.stabilizers,
            code.logical_basis,
            gauge_basis=gauge_basis,
            # anti_stabilizers=anti_stabilizers,
        )
        assert tuple(code.stabilizers) == tuple(clone.stabilizers)
        assert tuple(code.logical_basis) == tuple(clone.logical_basis)
        assert code.is_equivalent_to(clone)
        if gauge_basis is not None:
            assert tuple(code.gauge_basis) == tuple(gauge_basis)
        # if anti_stabilizers is not None:
        #     assert tuple(code.anti_stabilizers) == tuple(anti_stabilizers)

    assert_clone()
    assert_clone(gauge_basis=code.gauge_basis)
    # assert_clone(gauge_basis=code.gauge_basis, anti_stabilizers=code.anti_stabilizers)


def assert_group_property_consistency_of(code: SubsystemCode) -> None:
    assert code.stabilizer.generators == code.stabilizers
    assert code.anti_stabilizer.generators == code.anti_stabilizers
    assert code.logical.generators == code.logical_basis
    assert code.gauge.generators == code.gauge_basis


def assert_encoding_clifford_of(code: SubsystemCode) -> None:
    support = sorted(code.support)
    encoding_clifford = encoding_clifford_of(code, supported_by=support)
    assert encoding_clifford.is_valid
    images = clifford_images_of(code)
    assert len(images) == 2 * len(support)

    preimages = interleave(
        [RustSparsePauli({index: "X"}) for index in range(len(support))],
        [RustSparsePauli({index: "Z"}) for index in range(len(support))],
    )
    for preimage, image in zip_longest(preimages, images):
        dense_image = encoding_clifford.image_of(preimage)
        remapped = (
            Pauli({support[i]: dense_image[i] for i in dense_image.support})
            * identity(dense_image.phase)
        )
        assert image == remapped


def assert_code_generators(code: SubsystemCode) -> None:
    assert all(map(code.is_trivial_error, code.stabilizers))


def assert_valid_logical_basis(code: SubsystemCode) -> None:
    assert len(code.logical_basis) == 2 * code.logical_qubit_count
    for logical in code.logical_basis:
        assert logical * logical == Pauli.identity()
        for generator in code.stabilizers:
            assert generator.commutes_with(logical)
    assert PauliGroup(code.logical_basis).binary_rank == len(code.logical_basis)


def assert_valid_logical_actions(code: SubsystemCode) -> None:
    for index, logicals in enumerate(chunked(code.logical_basis, 2)):
        logical_x, logical_z = logicals
        for generator, phase in product(code.stabilizers, [1, -1, 1.0j, -1.0j]):
            x_action = code.logical_action_of(logical_x * generator * identity(phase))
            z_action = code.logical_action_of(logical_z * generator * identity(phase))
            assert x_action == Pauli({index: "X"}) * identity(phase)
            assert z_action == Pauli({index: "Z"}) * identity(phase)


def assert_valid_representatives(code: SubsystemCode) -> None:
    return
    for pauli in PauliEnumerator(set(range(code.logical_qubit_count))).up_to_weight(2):
        assert code.logical_action_of(code.representative_of(pauli)) == pauli


def assert_anti_generators(code: SubsystemCode) -> None:
    _validate_anti_stabilizers(
        code.anti_stabilizers, code.stabilizers, code.logical_basis
    )
