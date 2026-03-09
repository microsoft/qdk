# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Optional, List, Any

class NoisySimulatorError(BaseException):
    """
    EXPERIMENTAL:

    An error returned from the Q# noisy simulator.
    """

    ...

class Operation:
    """
    EXPERIMENTAL:

    This struct represents a quantum operation. A quantum operation is a linear
    transformation that maps a valid density matrix to another valid density matrices.
    """

    def __init__(self, kraus_operators: Any) -> None:
        """
        Construct an operation from a list of Kraus operators.
        Matrices must be of dimension 2^k x 2^k, where k is an integer.
        Raises a `NoisySimulatorError` if the Kraus matrices are ill formed.

        Input:
            kraus_operators: List[List[List[complex]]], can be a Python list or a numpy array.
        """
        ...

    def get_effect_matrix(self) -> List[List[complex]]:
        r"""
        Returns effect matrix:
        $$ (\sum_i K_i^{\dagger} K_i) $$
        where $K_i$ are Kraus operators.
        """
        ...

    def get_operation_matrix(self) -> List[List[complex]]:
        r"""
        Return matrix representation:
        $$ \sum_i K_i \otimes K_{i}* $$
        where $K_i$ are Kraus operators.
        """
        ...

    def get_kraus_operators(self) -> List[List[List[complex]]]:
        """
        Return list of Kraus operators.
        """
        ...

    def get_number_of_qubits(self) -> int:
        """
        Return the number of qubits that the operation acts on.
        """

class Instrument:
    """
    EXPERIMENTAL:

    An instrument is the means by which we make measurements on a quantum system.
    """

    def __init__(self, operations: List[Operation]) -> None:
        """
        Constructs an instrument from a list of operations.
        """
        ...

class DensityMatrix:
    """
    EXPERIMENTAL:

    A square complex matrix of size 2^k x 2^k representing the state
    of a quantum system. The data is stored in a linear vector for
    performance reasons.
    """

    def data(self) -> List[List[complex]]:
        """
        Returns a copy of the matrix data.
        """
        ...

    def dimension(self) -> int:
        """
        Returns the dimension of the matrix. E.g.: if the matrix is
        5 x 5, it returns 5.
        """
        ...

    def number_of_qubits(self) -> int:
        """
        Returns the number of qubits in the system.
        """
        ...

class DensityMatrixSimulator:
    """
    EXPERIMENTAL:

    A quantum circuit simulator using a density matrix.

    If the simulator reaches an invalid state due to a numerical
    error, it will raise a `SimulatorException`.
    """

    def __init__(self, number_of_qubits: int, seed: Optional[int]) -> None:
        """
        Creates a new `DensityMatrixSimulator`.
        """
        ...

    def apply_operation(self, operation: Operation, qubits: List[int]) -> None:
        """
        Apply an operation to the given qubit ids.
        """
        ...

    def apply_instrument(self, instrument: Instrument, qubits: List[int]) -> None:
        """
        Apply non selective evolution to the given qubit ids.
        """
        ...

    def sample_instrument(self, instrument: Instrument, qubits: List[int]) -> int:
        """
        Performs selective evolution under the given instrument.
        Returns the index of the observed outcome.

        Use this method to perform measurements on the quantum system.
        """

    def get_state(self) -> Optional[DensityMatrix]:
        """
        Returns the `DensityMatrix` if the simulator is in a valid state,
        otherwise returns None.
        """
        ...

    def set_state(self, state: DensityMatrix) -> None:
        """
        Set state of the quantum system to another `DensityMatrix` of the
        same dimensions.
        """
        ...

    def set_trace(self, trace: float) -> None:
        """
        Set trace of the quantum system. That is, the probability of
        finding the quantum system in the current state. The new trace
        must be a number between 0 and 1.
        """
        ...

class StateVector:
    """
    EXPERIMENTAL:

    A vector representing a pure state of a quantum system.
    """

    def data(self) -> List[complex]:
        """
        Returns a copy of the vector data.
        """
        ...

    def dimension(self) -> int:
        """
        Returns the dimension of the vector.
        """
        ...

    def number_of_qubits(self) -> int:
        """
        Returns the number of qubits in the system.
        """
        ...

class StateVectorSimulator:
    """
    EXPERIMENTAL:

    A quantum circuit simulator using a density matrix.

    If the simulator reaches an invalid state due to a numerical
    error, it will raise a `SimulatorException`.
    """

    def __init__(self, number_of_qubits: int, seed: Optional[int]) -> None:
        """
        Creates a new `DensityMatrixSimulator`.
        """
        ...

    def apply_operation(self, operation: Operation, qubits: List[int]) -> None:
        """
        Apply an operation to the given qubit ids.
        """
        ...

    def apply_instrument(self, instrument: Instrument, qubits: List[int]) -> None:
        """
        Apply non selective evolution to the given qubit ids.
        """
        ...

    def sample_instrument(self, instrument: Instrument, qubits: List[int]) -> int:
        """
        Performs selective evolution under the given instrument.
        Returns the index of the observed outcome.

        Use this method to perform measurements on the quantum system.
        """

    def get_state(self) -> Optional[StateVector]:
        """
        Returns the `StateVector` if the simulator is in a valid state,
        otherwise returns None.
        """
        ...

    def set_state(self, state: StateVector) -> None:
        """
        Set state of the quantum system to another `StateVector` of the
        same dimensions.
        """
        ...

    def set_trace(self, trace: float) -> None:
        """
        Set trace of the quantum system. That is, the probability of
        finding the quantum system in the current state. The new trace
        must be a number between 0 and 1.
        """
        ...
