# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""One-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 1D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin chain
simulations and other one-dimensional quantum systems.
"""

from qsharp.magnets.geometry.hypergraph import Hyperedge, Hypergraph


class Chain1D(Hypergraph):
    """A one-dimensional open chain lattice.

    Represents a linear chain of vertices with nearest-neighbor edges.
    The chain has open boundary conditions, meaning the first and last
    vertices are not connected.

    Edges are colored for parallel updates:
    - Color -1 (if self_loops): Self-loop edges on each vertex
    - Color 0: Even-indexed nearest-neighbor edges (0-1, 2-3, ...)
    - Color 1: Odd-indexed nearest-neighbor edges (1-2, 3-4, ...)

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

        # Update color for self-loop edges
        if self_loops:
            for i in range(length):
                self.color[(i,)] = -1

        for i in range(length - 1):
            color = i % 2
            self.color[(i, i + 1)] = color

        self.length = length


class Ring1D(Hypergraph):
    """A one-dimensional ring (periodic chain) lattice.

    Represents a circular chain of vertices with nearest-neighbor edges.
    The ring has periodic boundary conditions, meaning the first and last
    vertices are connected.

    Edges are colored for parallel updates:
    - Color -1 (if self_loops): Self-loop edges on each vertex
    - Color 0: Even-indexed nearest-neighbor edges (0-1, 2-3, ...)
    - Color 1: Odd-indexed nearest-neighbor edges (1-2, 3-4, ...)

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

        # Update color for self-loop edges
        if self_loops:
            for i in range(length):
                self.color[(i,)] = -1

        for i in range(length):
            j = (i + 1) % length
            color = i % 2
            self.color[tuple(sorted([i, j]))] = color

        self.length = length
