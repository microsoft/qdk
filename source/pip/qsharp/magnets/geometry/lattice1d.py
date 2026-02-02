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

    The edges are partitioned into two parts for parallel updates:
    - Part 0 (if self_loops): Self-loop edges on each vertex
    - Part 1: Even-indexed nearest-neighbor edges (0-1, 2-3, ...)
    - Part 2: Odd-indexed nearest-neighbor edges (1-2, 3-4, ...)

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

        # Set up edge partitions for parallel updates
        if self_loops:
            self.parts = [list(range(length - 1))]
        else:
            self.parts = []

        self.parts.append(list(range(0, length - 1, 2)))
        self.parts.append(list(range(1, length - 1, 2)))

        self.length = length


class Ring1D(Hypergraph):
    """A one-dimensional ring (periodic chain) lattice.

    Represents a circular chain of vertices with nearest-neighbor edges.
    The ring has periodic boundary conditions, meaning the first and last
    vertices are connected.

    The edges are partitioned into two parts for parallel updates:
    - Part 0 (if self_loops): Self-loop edges on each vertex
    - Part 1: Even-indexed nearest-neighbor edges (0-1, 2-3, ...)
    - Part 2: Odd-indexed nearest-neighbor edges (1-2, 3-4, ...)

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

        # Set up edge partitions for parallel updates
        if self_loops:
            self.parts = [list(range(length))]
        else:
            self.parts = []

        self.parts.append(list(range(0, length, 2)))
        self.parts.append(list(range(1, length, 2)))

        self.length = length
