# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from qdk.simulation import (
    NoisySimulatorError,
    Operation,
    Instrument,
    DensityMatrixSimulator,
    StateVectorSimulator,
)
import pytest

# Tests for the Q# noisy simulator.


def test_matrices_are_not_transposed_when_returned_back_to_python():
    """
    This test is due to the performance optimization we make in
    `noisy_simulator/src/operation.rs/Operation::new`, we want to check that
    we are reversing the transpose we made there before returning to Python.
    """
    # This is one of the Kraus operators of the depolarizing channel,
    # so it is a valid Kraus operator.
    op = Operation([[[0j, -0.5j], [0.5j, 0j]]])
    assert op.get_kraus_operators() == [[[0j, -0.5j], [0.5j, 0j]]]


# Operation tests


def test_operation_number_of_qubits_is_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    assert op.get_number_of_qubits() == 1


def test_operation_kraus_operators_are_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    assert op.get_kraus_operators() == [[[(1 + 0j), 0j], [0j, 0j]]]


def test_operation_effect_matrix_is_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    assert op.get_effect_matrix() == [[(1 + 0j), 0j], [0j, 0j]]


def test_operation_matrix_is_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    assert op.get_operation_matrix() == [
        [(1 + 0j), 0j, 0j, 0j],
        [0j, 0j, 0j, 0j],
        [0j, 0j, 0j, 0j],
        [0j, 0j, 0j, 0j],
    ]


def test_constructing_an_empty_operation_throws_exception():
    with pytest.raises(NoisySimulatorError) as excinfo:
        _ = Operation([])
    assert (
        str(excinfo.value)
        == "error when building operation: there should be at least one Kraus Operator"
    )


def test_constructed_an_ill_formed_operation_throws_exception():
    with pytest.raises(NoisySimulatorError) as excinfo:
        op = Operation([[[1, 0], [0, 0, 0]]])
    assert str(excinfo.value) == "ill formed matrix, all rows should be the same length"


# Instrument tests


def test_constructing_an_instrument_with_a_valid_operation_succeeds():
    op = Operation([[[1, 0], [0, 0]]])
    _ = Instrument([op])


def test_constructing_an_ill_formed_instrument_throws_exception():
    op0 = Operation([[[1, 0], [0, 0]]])
    op1 = Operation(
        [
            [
                [1, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
            ]
        ]
    )

    with pytest.raises(NoisySimulatorError) as excinfo:
        _ = Instrument([op0, op1])

    assert (
        str(excinfo.value)
        == "error when building instrument: all Operations should target the same number of qubits"
    )


def test_constructing_an_empty_instrument_throws_exception():
    with pytest.raises(NoisySimulatorError) as excinfo:
        inst = Instrument([])
    assert (
        str(excinfo.value)
        == "error when building instrument: there should be at least one Operation"
    )


# DensityMatrixSimulator tests


def test_density_matrix_simulator_apply_operation_is_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    sim = DensityMatrixSimulator(1, seed=42)
    sim.apply_operation(op, [0])


def test_density_matrix_simulator_apply_instrument_is_mapped_correctly():
    mz0 = Operation([[[1, 0], [0, 0]]])
    mz = Instrument([mz0])
    sim = DensityMatrixSimulator(1, seed=42)
    sim.apply_instrument(mz, [0])


def test_density_matrix_simulator_sample_instrument_is_mapped_correctly():
    mz0 = Operation([[[1, 0], [0, 0]]])
    mz = Instrument([mz0])
    sim = DensityMatrixSimulator(1, seed=42)
    assert 0 == sim.sample_instrument(mz, [0])


def test_density_matrix_simulator_get_state_is_mapped_correctly():
    sim = DensityMatrixSimulator(1)
    assert sim.get_state().data() == [[1, 0], [0, 0]]


def test_density_matrix_simulator_set_state_is_mapped_correctly():
    f = 0.5**0.5
    h = Operation([[[f, f], [f, -f]]])
    sim = DensityMatrixSimulator(1)
    inital_state = sim.get_state()
    sim.apply_operation(h, [0])
    sim.set_state(inital_state)
    assert sim.get_state().data() == [[1, 0], [0, 0]]


def test_density_matrix_simulator_set_trace_is_mapped_correctly():
    sim = DensityMatrixSimulator(1)
    sim.set_trace(0.5)


def test_density_matrix_simulator_out_of_bounds_qubit():
    f = 0.5**0.5
    h = Operation([[[f, f], [f, -f]]])
    sim = DensityMatrixSimulator(1)

    with pytest.raises(NoisySimulatorError) as excinfo:
        sim.apply_operation(h, [1])

    assert str(excinfo.value) == "qubit id out of bounds: 1"


# StateVectorSimulator tests


def test_state_vector_simulator_apply_operation_is_mapped_correctly():
    op = Operation([[[1, 0], [0, 0]]])
    sim = StateVectorSimulator(1, seed=42)
    sim.apply_operation(op, [0])


def test_state_vector_simulator_apply_instrument_is_mapped_correctly():
    mz0 = Operation([[[1, 0], [0, 0]]])
    mz = Instrument([mz0])
    sim = StateVectorSimulator(1, seed=42)
    sim.apply_instrument(mz, [0])


def test_state_vector_simulator_sample_instrument_is_mapped_correctly():
    mz0 = Operation([[[1, 0], [0, 0]]])
    mz = Instrument([mz0])
    sim = StateVectorSimulator(1, seed=42)
    assert 0 == sim.sample_instrument(mz, [0])


def test_state_vector_simulator_get_state_is_mapped_correctly():
    sim = StateVectorSimulator(1)
    assert sim.get_state().data() == [1, 0]


def test_state_vector_simulator_set_state_is_mapped_correctly():
    f = 0.5**0.5
    h = Operation([[[f, f], [f, -f]]])
    sim = StateVectorSimulator(1)
    inital_state = sim.get_state()
    sim.apply_operation(h, [0])
    sim.set_state(inital_state)
    assert sim.get_state().data() == [1, 0]


def test_state_vector_simulator_set_trace_is_mapped_correctly():
    sim = StateVectorSimulator(1)
    sim.set_trace(0.5)


def test_state_vector_simulator_out_of_bounds_qubit():
    f = 0.5**0.5
    h = Operation([[[f, f], [f, -f]]])
    sim = StateVectorSimulator(1)

    with pytest.raises(NoisySimulatorError) as excinfo:
        sim.apply_operation(h, [1])

    assert str(excinfo.value) == "qubit id out of bounds: 1"
