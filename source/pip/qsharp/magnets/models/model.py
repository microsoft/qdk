# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportPrivateImportUsage=false

"""Base Model class for quantum spin models.

This module provides the base class for representing quantum spin models
as Hamiltonians. The Model class integrates with hypergraph geometries
to define interaction topologies and stores coefficients for each edge.
"""

from qsharp.magnets.geometry import Hyperedge, Hypergraph


class Model:
    """Base class for quantum spin models.

    This class represents a quantum spin Hamiltonian defined on a hypergraph
    geometry. The Hamiltonian is characterized by:

    - Coefficients: A mapping from edge vertex tuples to float coefficients
    - Terms: Groupings of hyperedges for Trotterization or parallel execution

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
        >>> model.get_coefficient((0, 1))
        1.5
    """

    def __init__(self, geometry: Hypergraph):
        """Initialize the Model.

        Creates a quantum spin model on the given geometry. The model starts
        with all coefficients set to zero and no term groupings.

        Args:
            geometry: Hypergraph defining the interaction topology. The number
                of vertices determines the number of qubits in the model.
        """
        self.geometry: Hypergraph = geometry
        self._qubits: set[int] = set()
        self._coefficients: dict[tuple[int, ...], float] = dict()
        for edge in geometry.edges():
            self._qubits.update(edge.vertices)
            self._coefficients[edge.vertices] = 0.0
        self._terms: list[list[Hyperedge]] = []

    def set_coefficient(self, vertices: tuple[int, ...], value: float) -> None:
        """Set the coefficient for an edge in the Hamiltonian.

        Args:
            vertices: Tuple of vertex indices identifying the edge.
            value: The coefficient value to set.

        Raises:
            KeyError: If the vertex tuple does not correspond to an edge
                in the geometry.
        """
        if vertices not in self._coefficients:
            raise KeyError(f"No edge with vertices {vertices} in geometry")
        self._coefficients[vertices] = value

    def get_coefficient(self, vertices: tuple[int, ...]) -> float:
        """Get the coefficient for an edge in the Hamiltonian.

        Args:
            vertices: Tuple of vertex indices identifying the edge.

        Returns:
            The coefficient value for the specified edge.

        Raises:
            KeyError: If the vertex tuple does not correspond to an edge
                in the geometry.
        """
        return self._coefficients[vertices]

    def add_term(self, edges: list[Hyperedge]) -> None:
        """Add a term grouping to the model.

        Appends a list of hyperedges as a term. Terms are used for
        grouping edges for Trotterization or parallel execution.

        Args:
            edges: List of Hyperedge objects to group as a term.
        """
        self._terms.append(list(edges))

    def terms(self) -> list[list[Hyperedge]]:
        """Return the list of term groupings.

        Returns:
            List of lists of Hyperedges representing term groupings.
        """
        return self._terms

    def __str__(self) -> str:
        """String representation of the model."""
        return "Generic model with {} terms on {} qubits.".format(
            len(self._terms), len(self._qubits)
        )

    def __repr__(self) -> str:
        """String representation of the model."""
        return self.__str__()


def translation_invariant_ising_model(
    geometry: Hypergraph, h: float, J: float
) -> Model:
    """Create a translation-invariant Ising model on the given geometry.

    The Hamiltonian is:
        H = -J * Σ_{<i,j>} Z_i Z_j - h * Σ_i X_i

    Two-body edges (len=2) in the geometry represent ZZ interactions with
    coefficient -J. Single-vertex edges (len=1) represent X field terms
    with coefficient -h. Edges are grouped into terms by their color
    for parallel execution.

    Args:
        geometry: The Hypergraph defining the interaction topology.
            Should include single-vertex edges for field terms.
        h: The transverse field strength (coefficient for X terms).
        J: The coupling strength (coefficient for ZZ interaction terms).

    Returns:
        A Model instance representing the Ising Hamiltonian.
    """
    model = Model(geometry)
    model._terms = [
        [] for _ in range(geometry.ncolors + 1)
    ]  # Initialize term groupings based on edge colors
    for edge in geometry.edges():
        vertices = edge.vertices
        if len(vertices) == 1:
            model.set_coefficient(vertices, -h)  # Set X field coefficient
        elif len(vertices) == 2:
            model.set_coefficient(vertices, -J)  # Set ZZ interaction coefficient
        color = geometry.color[vertices]
        model._terms[color].append(edge)  # Group edges by color for parallel execution

    return model
