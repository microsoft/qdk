# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Two-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 2D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin system
simulations and other two-dimensional quantum systems.
"""

from qsharp.magnets.utilities import Hyperedge, Hypergraph


class Patch2D(Hypergraph):
    """A two-dimensional open rectangular lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges.
    The patch has open boundary conditions, meaning edges do not wrap around.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    Edges are colored for parallel updates:
    - Color -1 (if self_loops): Self-loop edges on each vertex
    - Color 0: Even-column horizontal edges
    - Color 1: Odd-column horizontal edges
    - Color 2: Even-row vertical edges
    - Color 3: Odd-row vertical edges

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
        self.width = width
        self.height = height

        if self_loops:
            _edges = [Hyperedge([i]) for i in range(width * height)]
        else:
            _edges = []

        # Horizontal edges (connecting (x, y) to (x+1, y))
        for y in range(height):
            for x in range(width - 1):
                _edges.append(Hyperedge([self._index(x, y), self._index(x + 1, y)]))

        # Vertical edges (connecting (x, y) to (x, y+1))
        for y in range(height - 1):
            for x in range(width):
                _edges.append(Hyperedge([self._index(x, y), self._index(x, y + 1)]))
        super().__init__(_edges)

        # Set up edge colors for parallel updates
        if self_loops:
            for i in range(width * height):
                self.color[(i,)] = -1

        # Color horizontal edges
        for y in range(height):
            for x in range(width - 1):
                v1, v2 = self._index(x, y), self._index(x + 1, y)
                color = 0 if x % 2 == 0 else 1
                self.color[tuple(sorted([v1, v2]))] = color

        # Color vertical edges
        for y in range(height - 1):
            for x in range(width):
                v1, v2 = self._index(x, y), self._index(x, y + 1)
                color = 2 if y % 2 == 0 else 3
                self.color[tuple(sorted([v1, v2]))] = color

    def _index(self, x: int, y: int) -> int:
        """Convert (x, y) coordinates to vertex index."""
        return y * self.width + x


class Torus2D(Hypergraph):
    """A two-dimensional toroidal (periodic) lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges
    and periodic boundary conditions in both directions. The topology is
    that of a torus.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    Edges are colored for parallel updates:
    - Color -1 (if self_loops): Self-loop edges on each vertex
    - Color 0: Even-column horizontal edges
    - Color 1: Odd-column horizontal edges
    - Color 2: Even-row vertical edges
    - Color 3: Odd-row vertical edges

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
        self.width = width
        self.height = height

        if self_loops:
            _edges = [Hyperedge([i]) for i in range(width * height)]
        else:
            _edges = []

        # Horizontal edges (connecting (x, y) to ((x+1) % width, y))
        for y in range(height):
            for x in range(width):
                _edges.append(
                    Hyperedge([self._index(x, y), self._index((x + 1) % width, y)])
                )

        # Vertical edges (connecting (x, y) to (x, (y+1) % height))
        for y in range(height):
            for x in range(width):
                _edges.append(
                    Hyperedge([self._index(x, y), self._index(x, (y + 1) % height)])
                )

        super().__init__(_edges)

        # Set up edge colors for parallel updates
        if self_loops:
            for i in range(width * height):
                self.color[(i,)] = -1

        # Color horizontal edges
        for y in range(height):
            for x in range(width):
                v1, v2 = self._index(x, y), self._index((x + 1) % width, y)
                color = 0 if x % 2 == 0 else 1
                self.color[tuple(sorted([v1, v2]))] = color

        # Color vertical edges
        for y in range(height):
            for x in range(width):
                v1, v2 = self._index(x, y), self._index(x, (y + 1) % height)
                color = 2 if y % 2 == 0 else 3
                self.color[tuple(sorted([v1, v2]))] = color

    def _index(self, x: int, y: int) -> int:
        """Convert (x, y) coordinates to vertex index."""
        return y * self.width + x
