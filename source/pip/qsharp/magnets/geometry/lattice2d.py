# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Two-dimensional lattice geometries for quantum simulations.

This module provides classes for representing 2D lattice structures as
hypergraphs. These lattices are commonly used in quantum spin system
simulations and other two-dimensional quantum systems.
"""

from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    edge_coloring as default_edge_coloring,
)


class Patch2D(Hypergraph):
    """A two-dimensional open rectangular lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges.
    The patch has open boundary conditions, meaning edges do not wrap around.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    Attributes:
        width: Number of vertices in the horizontal direction.
        height: Number of vertices in the vertical direction.

    Example:

    .. code-block:: python
        >>> patch = Patch2D(3, 2)
        >>> str(patch)
        '3x2 lattice patch with 6 vertices and 7 edges'
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

    def _index(self, x: int, y: int) -> int:
        """Convert (x, y) coordinates to vertex index."""
        return y * self.width + x

    def __str__(self) -> str:
        """Return the summary string ``"{width}x{height} lattice patch with {nvertices} vertices and {nedges} edges"``."""
        return f"{self.width}x{self.height} lattice patch with {self.nvertices} vertices and {self.nedges} edges"

    def __repr__(self) -> str:
        """Return a string representation of the Patch2D geometry."""
        return f"Patch2D(width={self.width}, height={self.height})"


class Torus2D(Hypergraph):
    """A two-dimensional toroidal (periodic) lattice.

    Represents a rectangular grid of vertices with nearest-neighbor edges
    and periodic boundary conditions in both directions. The topology is
    that of a torus.

    Vertices are indexed in row-major order: vertex (x, y) has index y * width + x.

    Attributes:
        width: Number of vertices in the horizontal direction.
        height: Number of vertices in the vertical direction.

    Example:

    .. code-block:: python
        >>> torus = Torus2D(3, 2)
        >>> str(torus)
        '3x2 lattice torus with 6 vertices and 12 edges'
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

    def _index(self, x: int, y: int) -> int:
        """Convert (x, y) coordinates to vertex index."""
        return y * self.width + x

    def __str__(self) -> str:
        """Return the summary string ``"{width}x{height} lattice torus with {nvertices} vertices and {nedges} edges"``."""
        return f"{self.width}x{self.height} lattice torus with {self.nvertices} vertices and {self.nedges} edges"

    def __repr__(self) -> str:
        """Return a string representation of the Torus2D geometry."""
        return f"Torus2D(width={self.width}, height={self.height})"


def edge_coloring(hypergraph: Hypergraph) -> HypergraphEdgeColoring:
    """Compute edge coloring for 2D lattice geometries with fallback behavior.

    - ``Patch2D``: uses parity-based 4-coloring for horizontal/vertical edges,
      with ``-1`` for self-loops.
    - ``Torus2D``: attempts the same structured coloring; if periodic parity
      conflicts arise (e.g., odd dimensions), falls back to default coloring.
    - Other ``Hypergraph`` types: delegates to default hypergraph coloring.
    """
    if isinstance(hypergraph, Patch2D):
        coloring = HypergraphEdgeColoring(hypergraph)
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
                continue

            u, v = edge.vertices
            x_u, y_u = u % hypergraph.width, u // hypergraph.width
            x_v, y_v = v % hypergraph.width, v // hypergraph.width

            if y_u == y_v:
                color = 0 if min(x_u, x_v) % 2 == 0 else 1
            else:
                color = 2 if min(y_u, y_v) % 2 == 0 else 3
            coloring.add_edge(edge, color)
        return coloring

    if isinstance(hypergraph, Torus2D):
        coloring = HypergraphEdgeColoring(hypergraph)
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)
                continue

            u, v = edge.vertices
            x_u, y_u = u % hypergraph.width, u // hypergraph.width
            x_v, y_v = v % hypergraph.width, v // hypergraph.width

            if y_u == y_v:
                if {x_u, x_v} == {0, hypergraph.width - 1}:
                    color = 1 if hypergraph.width % 2 == 0 else 4
                else:
                    color = 0 if min(x_u, x_v) % 2 == 0 else 1
            else:
                if {y_u, y_v} == {0, hypergraph.height - 1}:
                    color = 3 if hypergraph.height % 2 == 0 else 5
                else:
                    color = 2 if min(y_u, y_v) % 2 == 0 else 3
            coloring.add_edge(edge, color)
        return coloring

    return default_edge_coloring(hypergraph)
