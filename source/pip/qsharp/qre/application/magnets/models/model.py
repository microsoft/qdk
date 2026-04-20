# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

from collections.abc import Sequence
from typing import Optional

from ..utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    PauliString,
)

"""Base Model class for quantum spin models.

This module provides the base class for representing quantum spin models
as Hamiltonians. The Model class integrates with hypergraph geometries
to define interaction topologies and stores coefficients for each edge.
"""


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

        The model stores operators lazily in ``_ops`` as interaction operators
        are defined. Noncommuting collections of operators are collected in
        ``_terms`` that stores the indices of its interaction operators. This
        list of arrays seperate terms into parallizable groups by color. It is
        initialized as one empty term group.

        Args:
            geometry: Hypergraph defining the interaction topology. The number
                of vertices determines the number of qubits in the model.
        """
        self.geometry: Hypergraph = geometry
        self._qubits: set[int] = set()
        self._ops: list[PauliString] = []
        for edge in geometry.edges():
            self._qubits.update(edge.vertices)
        self._terms: dict[int, dict[int, list[int]]] = {}

    def add_interaction(
        self,
        edge: Hyperedge,
        pauli_string: Sequence[int | str] | str,
        coefficient: complex = 1.0,
        term: Optional[int] = None,
        color: int = 0,
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
                self._terms[term] = {}
            if color not in self._terms[term]:
                self._terms[term][color] = []
            self._terms[term][color].append(len(self._ops) - 1)

    @property
    def nqubits(self) -> int:
        """Return the number of qubits in the model."""
        return len(self._qubits)

    @property
    def nterms(self) -> int:
        """Return the number of term groups in the model."""
        return len(self._terms)

    @property
    def terms(self) -> list[int]:
        """Get the list of term indices in the model."""
        return list(self._terms.keys())

    def ncolors(self, term: int) -> int:
        """Return the number of colors in a given term."""
        if term not in self._terms:
            raise ValueError(f"Term {term} does not exist in the model.")
        return len(self._terms[term])

    def colors(self, term: int) -> list[int]:
        """Return the list of colors in a given term."""
        if term not in self._terms:
            raise ValueError(f"Term {term} does not exist in the model.")
        return list(self._terms[term].keys())

    def nops(self, term: int, color: int) -> int:
        """Return the number of operators in a given term and color."""
        if term not in self._terms:
            raise ValueError(f"Term {term} does not exist in the model.")
        if color not in self._terms[term]:
            raise ValueError(f"Color {color} does not exist in term {term}.")
        return len(self._terms[term][color])

    def ops(self, term: int, color: int) -> list[PauliString]:
        """Return the list of operators in a given term and color."""
        if term not in self._terms:
            raise ValueError(f"Term {term} does not exist in the model.")
        if color not in self._terms[term]:
            raise ValueError(f"Color {color} does not exist in term {term}.")
        return [self._ops[i] for i in self._terms[term][color]]

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
    - Terms are grouped into two groups: ``0`` for field terms and ``1`` for coupling terms.
    """

    def __init__(self, geometry: Hypergraph, h: float, J: float):
        super().__init__(geometry)
        self.h = h
        self.J = J
        self._terms = {0: {}, 1: {}}

        coloring: HypergraphEdgeColoring = geometry.edge_coloring()
        for edge in geometry.edges():
            vertices = edge.vertices
            if len(vertices) == 1:
                self.add_interaction(edge, "X", -h, term=0, color=0)
            elif len(vertices) == 2:
                color = coloring.color(edge.vertices)
                if color is None:
                    raise ValueError("Geometry edge coloring failed to assign a color.")
                self.add_interaction(edge, "ZZ", -J, term=1, color=color)

    def __str__(self) -> str:
        return (
            f"Ising model with {self.nterms} terms on {self.nqubits} qubits "
            f"(h={self.h}, J={self.J})."
        )

    def __repr__(self) -> str:
        return (
            f"IsingModel(nqubits={self.nqubits}, nterms={self.nterms}, "
            f"h={self.h}, J={self.J})"
        )


class HeisenbergModel(Model):
    """Translation-invariant Heisenberg model on a hypergraph geometry.

    The Hamiltonian is:
        H = -J * Σ_{<i,j>} (X_i X_j + Y_i Y_j + Z_i Z_j)

    - Two-vertex edges define XX, YY, and ZZ coupling terms with coefficient ``-J``.
    - Terms are grouped into three parts: ``0`` for XX, ``1`` for YY, and ``2`` for ZZ.
    """

    def __init__(self, geometry: Hypergraph, J: float):
        super().__init__(geometry)
        self.J = J
        self.coloring: HypergraphEdgeColoring = geometry.edge_coloring()
        self._terms = {0: {}, 1: {}, 2: {}}
        for edge in geometry.edges():
            vertices = edge.vertices
            if len(vertices) == 2:
                color = self.coloring.color(edge.vertices)
                if color is None:
                    raise ValueError("Geometry edge coloring failed to assign a color.")
                self.add_interaction(edge, "XX", -J, term=0, color=color)
                self.add_interaction(edge, "YY", -J, term=1, color=color)
                self.add_interaction(edge, "ZZ", -J, term=2, color=color)

    def __str__(self) -> str:
        return (
            f"Heisenberg model with {self.nterms} terms on {self.nqubits} qubits "
            f"(J={self.J})."
        )

    def __repr__(self) -> str:
        return (
            f"HeisenbergModel(nqubits={self.nqubits}, nterms={self.nterms}, "
            f"J={self.J})"
        )
