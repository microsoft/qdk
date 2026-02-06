# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Two-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 2D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin system
simulations and other two-dimensional quantum systems.
"""

from qsharp.magnets.geometry.hypergraph import Hyperedge, Hypergraph


class Patch2D(Hypergraph):
    """A two-dimensional open rectangular lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges.
    The patch has open boundary conditions, meaning edges do not wrap around.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    The edges are partitioned into parts for parallel updates:
    - Part 0 (if self_loops): Self-loop edges on each vertex
    - Part 1: Even-column horizontal edges
    - Part 2: Odd-column horizontal edges
    - Part 3: Even-row vertical edges
    - Part 4: Odd-row vertical edges

    Attributes:
        width: Number of vertices in the horizontal direction.
        height: Number of vertices in the vertical direction.

    Example:

    .. code-block:: python
        >>> patch = Patch2D(3, 2)
        >>> patch.nvertices
        6
        >>> patch.nedges
        7
    """

    def __init__(self, width: int, height: int, self_loops: bool = False) -> None:
        """Initialize a 2D patch lattice.

        Args:
            width: Number of vertices in the horizontal direction.
            height: Number of vertices in the vertical direction.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """

        def index(x: int, y: int) -> int:
            return y * width + x

        if self_loops:
            _edges = [Hyperedge([i]) for i in range(width * height)]
        else:
            _edges = []

        # Horizontal edges (connecting (x, y) to (x+1, y))
        horizontal_even = []
        horizontal_odd = []
        for y in range(height):
            for x in range(width - 1):
                edge_idx = len(_edges)
                _edges.append(Hyperedge([index(x, y), index(x + 1, y)]))
                if x % 2 == 0:
                    horizontal_even.append(edge_idx)
                else:
                    horizontal_odd.append(edge_idx)

        # Vertical edges (connecting (x, y) to (x, y+1))
        vertical_even = []
        vertical_odd = []
        for y in range(height - 1):
            for x in range(width):
                edge_idx = len(_edges)
                _edges.append(Hyperedge([index(x, y), index(x, y + 1)]))
                if y % 2 == 0:
                    vertical_even.append(edge_idx)
                else:
                    vertical_odd.append(edge_idx)

        super().__init__(_edges)

        # Set up edge partitions for parallel updates
        if self_loops:
            self.parts = [list(range(width * height))]
        else:
            self.parts = []

        self.parts.append(horizontal_even)
        self.parts.append(horizontal_odd)
        self.parts.append(vertical_even)
        self.parts.append(vertical_odd)

        self.width = width
        self.height = height


class Torus2D(Hypergraph):
    """A two-dimensional toroidal (periodic) lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges
    and periodic boundary conditions in both directions. The topology is
    that of a torus.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    The edges are partitioned into parts for parallel updates:
    - Part 0 (if self_loops): Self-loop edges on each vertex
    - Part 1: Even-column horizontal edges
    - Part 2: Odd-column horizontal edges
    - Part 3: Even-row vertical edges
    - Part 4: Odd-row vertical edges

    Attributes:
        width: Number of vertices in the horizontal direction.
        height: Number of vertices in the vertical direction.

    Example:

    .. code-block:: python
        >>> torus = Torus2D(3, 2)
        >>> torus.nvertices
        6
        >>> torus.nedges
        12
    """

    def __init__(self, width: int, height: int, self_loops: bool = False) -> None:
        """Initialize a 2D torus lattice.

        Args:
            width: Number of vertices in the horizontal direction.
            height: Number of vertices in the vertical direction.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """

        def index(x: int, y: int) -> int:
            return y * width + x

        if self_loops:
            _edges = [Hyperedge([i]) for i in range(width * height)]
        else:
            _edges = []

        # Horizontal edges (connecting (x, y) to ((x+1) % width, y))
        horizontal_even = []
        horizontal_odd = []
        for y in range(height):
            for x in range(width):
                edge_idx = len(_edges)
                _edges.append(Hyperedge([index(x, y), index((x + 1) % width, y)]))
                if x % 2 == 0:
                    horizontal_even.append(edge_idx)
                else:
                    horizontal_odd.append(edge_idx)

        # Vertical edges (connecting (x, y) to (x, (y+1) % height))
        vertical_even = []
        vertical_odd = []
        for y in range(height):
            for x in range(width):
                edge_idx = len(_edges)
                _edges.append(Hyperedge([index(x, y), index(x, (y + 1) % height)]))
                if y % 2 == 0:
                    vertical_even.append(edge_idx)
                else:
                    vertical_odd.append(edge_idx)

        super().__init__(_edges)

        # Set up edge partitions for parallel updates
        if self_loops:
            self.parts = [list(range(width * height))]
        else:
            self.parts = []

        self.parts.append(horizontal_even)
        self.parts.append(horizontal_odd)
        self.parts.append(vertical_even)
        self.parts.append(vertical_odd)

        self.width = width
        self.height = height
