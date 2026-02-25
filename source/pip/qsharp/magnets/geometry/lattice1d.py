# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""One-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 1D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin chain
simulations and other one-dimensional quantum systems.
"""

from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    edge_coloring as default_edge_coloring,
)


class Chain1D(Hypergraph):
    """A one-dimensional open chain lattice.

    Represents a linear chain of vertices with nearest-neighbor edges.
    The chain has open boundary conditions, meaning the first and last
    vertices are not connected.

    Attributes:
        length: Number of vertices in the chain.

    Example:

    .. code-block:: python
        >>> chain = Chain1D(4)
        >>> chain.nvertices
        4
        >>> chain.nedges
        3
    """

    def __init__(self, length: int, self_loops: bool = False) -> None:
        """Initialize a 1D chain lattice.

        Args:
            length: Number of vertices in the chain.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """
        if self_loops:
            _edges = [Hyperedge([i]) for i in range(length)]

        else:
            _edges = []

        for i in range(length - 1):
            _edges.append(Hyperedge([i, i + 1]))

        super().__init__(_edges)
        self.length = length


class Ring1D(Hypergraph):
    """A one-dimensional ring (periodic chain) lattice.

    Represents a circular chain of vertices with nearest-neighbor edges.
    The ring has periodic boundary conditions, meaning the first and last
    vertices are connected.

    Attributes:
        length: Number of vertices in the ring.

    Example:

    .. code-block:: python
        >>> ring = Ring1D(4)
        >>> ring.nvertices
        4
        >>> ring.nedges
        4
    """

    def __init__(self, length: int, self_loops: bool = False) -> None:
        """Initialize a 1D ring lattice.

        Args:
            length: Number of vertices in the ring.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """
        if self_loops:
            _edges = [Hyperedge([i]) for i in range(length)]
        else:
            _edges = []

        for i in range(length):
            _edges.append(Hyperedge([i, (i + 1) % length]))
        super().__init__(_edges)

        self.length = length


def edge_coloring(hypergraph: Hypergraph) -> HypergraphEdgeColoring:
    """Compute a valid edge coloring for 1D lattice geometries.

        This function specializes coloring for :class:`Chain1D` and
        :class:`Ring1D`, and falls back to the default hypergraph coloring
        algorithm for all other :class:`Hypergraph` instances.

        Behavior:

        - ``Chain1D``:
            - Self-loops (single-vertex edges) are assigned color ``-1``.
            - Two-vertex edges use parity coloring based on ``min(i, j) % 2``.
        - ``Ring1D``:
            - Self-loops are assigned color ``-1``.
            - Non-wrap edges use ``min(i, j) % 2``.
            - The wrap-around edge ``{0, length - 1}`` uses a dedicated color,
                ``(length % 2) + 1``, to avoid same-color conflicts.
        - Other ``Hypergraph`` subclasses:
            - Delegates to :func:`qsharp.magnets.utilities.edge_coloring`.

    Args:
        hypergraph: Hypergraph instance to color.

    Returns:
        A :class:`HypergraphEdgeColoring` for ``hypergraph``.

    Example:

    .. code-block:: python
        >>> chain = Chain1D(5)
        >>> coloring = edge_coloring(chain)
        >>> sorted(coloring.colors())
        [0, 1]
    """
    if isinstance(hypergraph, Chain1D):
        coloring = HypergraphEdgeColoring(hypergraph)
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
            else:
                i, j = edge.vertices
                color = min(i, j) % 2
                coloring.add_edge(edge, color)
        return coloring

    if isinstance(hypergraph, Ring1D):
        coloring = HypergraphEdgeColoring(hypergraph)
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
            else:
                i, j = edge.vertices
                if {i, j} == {0, hypergraph.length - 1}:
                    color = (hypergraph.length % 2) + 1
                else:
                    color = min(i, j) % 2
                coloring.add_edge(edge, color)
        return coloring

    return default_edge_coloring(hypergraph)
