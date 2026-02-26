# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

from collections.abc import Sequence
from typing import Optional


"""Base Model class for quantum spin models.

This module provides the base class for representing quantum spin models
as Hamiltonians. The Model class integrates with hypergraph geometries
to define interaction topologies and stores coefficients for each edge.
"""

from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    PauliString,
)


class Model:
    """Base class for quantum spin models.

    This class represents a quantum spin Hamiltonian defined on a hypergraph
    geometry. The Hamiltonian is characterized by:

    - Ops: A list of PauliStrings (one entry per interaction term)
    - Terms: Groupings of operator indices for Trotterization or parallel execution

    The model is built on a hypergraph geometry that defines which qubits
    interact with each other.

    Attributes:
        geometry: The Hypergraph defining the interaction topology.

    Example:

    .. code-block:: python
        >>> from qsharp.magnets.geometry import Chain1D
        >>> geometry = Chain1D(4)
        >>> model = Model(geometry)
        >>> model.set_coefficient((0, 1), 1.5)
        >>> model.set_pauli_string((0, 1), PauliString.from_qubits((0, 1), "ZZ"))
        >>> model.get_coefficient((0, 1))
        1.5
    """

    def __init__(self, geometry: Hypergraph):
        """Initialize the Model.

        Creates a quantum spin model on the given geometry.

        The model stores operators lazily in ``_ops`` as terms are defined.
        ``_terms`` is initialized with one empty term group.

        Args:
            geometry: Hypergraph defining the interaction topology. The number
                of vertices determines the number of qubits in the model.
        """
        self.geometry: Hypergraph = geometry
        self._qubits: set[int] = set()
        self._ops: list[PauliString] = []
        for edge in geometry.edges():
            self._qubits.update(edge.vertices)
        self._terms: dict[int, list[int]] = {}

    def add_interaction(
        self,
        edge: Hyperedge,
        pauli_string: Sequence[int | str] | str,
        coefficient: complex = 1.0,
        term: Optional[int] = None,
    ) -> None:
        """Add an interaction term to the model.

        Args:
            edge: The Hyperedge representing the qubits involved in the interaction.
            pauli_string: The PauliString operator for this interaction.
            coefficient: The complex coefficient multiplying this term (default 1.0).
        """
        if edge not in self.geometry.edges():
            raise ValueError("Edge is not part of the model geometry.")
        s = PauliString.from_qubits(edge.vertices, pauli_string, coefficient)
        self._ops.append(s)
        if term is not None:
            if term not in self._terms:
                self._terms[term] = []
            self._terms[term].append(len(self._ops) - 1)

    @property
    def nqubits(self) -> int:
        """Return the number of qubits in the model."""
        return len(self._qubits)

    @property
    def nterms(self) -> int:
        """Return the number of term groups in the model."""
        return len(self._terms)

    def __str__(self) -> str:
        """String representation of the model."""
        return "Generic model with {} terms on {} qubits.".format(
            len(self._terms), len(self._qubits)
        )

    def __repr__(self) -> str:
        """String representation of the model."""
        return self.__str__()


class IsingModel(Model):
    """Translation-invariant Ising model on a hypergraph geometry.

    The Hamiltonian is:
        H = -J * Σ_{<i,j>} Z_i Z_j - h * Σ_i X_i

    - Single-vertex edges define X-field terms with coefficient ``-h``.
    - Two-vertex edges define ZZ-coupling terms with coefficient ``-J``.
    - Terms are grouped into two groups: ``0`` for field terms and ``1`` for
      coupling terms.
    """

    def __init__(self, geometry: Hypergraph, h: float, J: float):
        super().__init__(geometry)
        self.coloring: HypergraphEdgeColoring = geometry.edge_coloring()
        self._terms = {0: [], 1: []}

        for edge in geometry.edges():
            vertices = edge.vertices
            if len(vertices) == 1:
                self.add_interaction(edge, "X", -h, term=0)
            elif len(vertices) == 2:
                self.add_interaction(edge, "ZZ", -J, term=1)
