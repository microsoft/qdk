# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""One-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 1D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin chain
simulations and other one-dimensional quantum systems.
"""

from typing import Optional

from ..utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
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

    def edge_coloring(
        self, seed: Optional[int] = 0, trials: int = 1
    ) -> HypergraphEdgeColoring:
        """Compute a valid edge coloring for this chain."""
        coloring = HypergraphEdgeColoring(self)
        for edge in self.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
            else:
                i, j = edge.vertices
                color = min(i, j) % 2
                coloring.add_edge(edge, color)
        return coloring


class MthNearestNeighborChain1D(Hypergraph):
    """An open chain with connections through the m-th nearest neighbor.

    Each edge is marked with one less than its neighbor distance. For example,
    edges between adjacent vertices have mark 0, and next-nearest-neighbor
    edges have mark 1.

    Attributes:
        length: Number of vertices in the chain.
        m: Maximum neighbor distance included in the chain.
    """

    def __init__(self, length: int, m: int, self_loops: bool = False) -> None:
        """Initialize an m-th nearest-neighbor 1D chain.

        Args:
            length: Number of vertices in the chain.
            m: Maximum neighbor distance to connect.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """
        if self_loops:
            _edges = [Hyperedge([i]) for i in range(length)]
        else:
            _edges = []

        for distance in range(1, min(m, length - 1) + 1):
            for i in range(length - distance):
                edge = Hyperedge([i, i + distance])
                edge.mark = distance - 1
                _edges.append(edge)

        super().__init__(_edges)
        self.length = length
        self.m = m

    def edge_coloring(
        self, seed: Optional[int] = 0, trials: int = 1
    ) -> HypergraphEdgeColoring:
        """Color each neighbor distance with two alternating colors."""
        coloring = HypergraphEdgeColoring(self)
        for edge in self.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
            else:
                i, j = edge.vertices
                distance = j - i
                color = 2 * (distance - 1) + (i // distance) % 2
                coloring.add_edge(edge, color)
        return coloring


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

    def edge_coloring(
        self, seed: Optional[int] = 0, trials: int = 1
    ) -> HypergraphEdgeColoring:
        """Compute a valid edge coloring for this ring."""
        coloring = HypergraphEdgeColoring(self)
        for edge in self.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
            else:
                i, j = edge.vertices
                if {i, j} == {0, self.length - 1}:
                    color = (self.length % 2) + 1
                else:
                    color = min(i, j) % 2
                coloring.add_edge(edge, color)
        return coloring
