# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

"""Base Model class for quantum spin models.

This module provides the base class for representing quantum spin models
as Hamiltonians built from Pauli operators. The Model class integrates
with hypergraph geometries to define interaction topologies and uses
Cirq's PauliString and PauliSum for representing quantum operators.
"""

from typing import Iterator
from qsharp.magnets.geometry import Hypergraph

try:
    from cirq import LineQubit, PauliSum, PauliString
except Exception as ex:
    raise ImportError(
        "qsharp.magnets.models requires the cirq extras. Install with 'pip install \"qsharp[cirq]\"'."
    ) from ex


class Model:
    """Base class for quantum spin models.

    This class wraps a list of cirq.PauliSum objects that define the Hamiltonian
    of a quantum system. Each element of the list represents a partition of
    the Hamiltonian into different terms, which is useful for:

    - Trotterization: Grouping commuting terms for efficient simulation
    - Parallel execution: Terms in the same partition can be applied simultaneously
    - Resource estimation: Analyzing different parts of the Hamiltonian separately

    The model is built on a hypergraph geometry that defines which qubits
    interact with each other. Subclasses should populate the `terms` list
    with appropriate PauliSum operators based on the geometry.

    Attributes:
        geometry: The Hypergraph defining the interaction topology.
        terms: List of PauliSum objects representing partitioned Hamiltonian terms.

    Example:

    .. code-block:: python
        >>> from qsharp.magnets.geometry import Chain1D
        >>> geometry = Chain1D(4)
        >>> model = Model(geometry)
        >>> model.add_term()  # Add an empty term
        >>> len(model.terms)
        1
    """

    def __init__(self, geometry: Hypergraph):
        """Initialize the Model.

        Creates a quantum spin model on the given geometry. The model starts
        with no Hamiltonian terms; subclasses or callers should add terms
        using `add_term()` and `add_to_term()`.

        Args:
            geometry: Hypergraph defining the interaction topology. The number
                of vertices determines the number of qubits in the model.
        """
        self.geometry: Hypergraph = geometry
        self._qubits: list[LineQubit] = [
            LineQubit(i) for i in range(geometry.nvertices)
        ]
        self.terms: list[PauliSum] = []

    def add_term(self, term: PauliSum = None) -> None:
        """Add a term to the Hamiltonian.

        Appends a new PauliSum to the list of Hamiltonian terms. This is
        typically used to create partitions for Trotterization, where each
        partition contains operators that can be applied together.

        Args:
            term: The PauliSum to add. If None, an empty PauliSum is added,
                which can be populated later using `add_to_term()`.
        """
        if term is None:
            term = PauliSum()
        self.terms.append(term)

    def add_to_term(self, index: int, pauli_string: PauliString) -> None:
        """Add a PauliString to a specific term in the Hamiltonian.

        Appends a Pauli operator (with coefficient) to an existing term.
        This is used to build up the Hamiltonian incrementally.

        Args:
            index: Index of the term to add to (0-indexed).
            pauli_string: The PauliString to add to the term. This can
                include a coefficient, e.g., `0.5 * cirq.Z(q0) * cirq.Z(q1)`.

        Raises:
            IndexError: If index is out of range of the terms list.
        """
        self.terms[index] += pauli_string

    def q(self, i: int) -> LineQubit:
        """Return the qubit at index i.

        Provides convenient access to qubits by their vertex index in
        the underlying geometry.

        Args:
            i: Index of the qubit (0-indexed, corresponds to vertex index).

        Returns:
            The LineQubit at the specified index.
        """
        return self._qubits[i]

    def qubit_list(self) -> list[LineQubit]:
        """Return the list of qubits in the model.

        Returns:
            A list of all LineQubit objects in the model, ordered by index.
        """
        return self._qubits

    def qubits(self) -> Iterator[LineQubit]:
        """Return an iterator over the qubits in the model.

        Returns:
            An iterator yielding LineQubit objects in index order.
        """
        return iter(self._qubits)

    def __str__(self) -> str:
        """String representation of the model."""
        return "Generic model with {} terms on {} qubits.".format(
            len(self.terms), len(self._qubits)
        )

    def __repr__(self) -> str:
        """String representation of the model."""
        return self.__str__()
