# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Hypergraph data structures for representing quantum system geometries.

This module provides classes for representing hypergraphs, which generalize
graphs by allowing edges (hyperedges) to connect any number of vertices.
Hypergraphs are useful for representing interaction terms in quantum
Hamiltonians, where multi-body interactions can involve more than two sites.
"""

from typing import Iterator, List


class Hyperedge:
    """A hyperedge connecting one or more vertices in a hypergraph.

    A hyperedge generalizes the concept of an edge in a graph. While a
    traditional edge connects exactly two vertices, a hyperedge can connect
    any number of vertices. This is useful for representing:
    - Single-site terms (self-loops): 1 vertex
    - Two-body interactions: 2 vertices
    - Multi-body interactions: 3+ vertices

    Attributes:
        vertices: Sorted list of vertex indices connected by this hyperedge.

    Example:
    
    .. code-block:: python
        >>> edge = Hyperedge([2, 0, 1])
        >>> edge.vertices
        [0, 1, 2]
    """

    def __init__(self, vertices: List[int]) -> None:
        """Initialize a hyperedge with the given vertices.

        Args:
            vertices: List of vertex indices. Will be sorted internally.
        """
        self.vertices: List[int] = sorted(vertices)

    def __repr__(self) -> str:
        return f"Hyperedge({self.vertices})"


class Hypergraph:
    """A hypergraph consisting of vertices connected by hyperedges.

    A hypergraph is a generalization of a graph where edges (hyperedges) can
    connect any number of vertices. This class serves as the base class for
    various lattice geometries used in quantum simulations.

    Attributes:
        _edges: List of hyperedges in the order they were added.
        _vertex_set: Set of all unique vertex indices in the hypergraph.
        _edge_list: Set of hyperedges for efficient membership testing.

    Example:
        >>> edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
        >>> graph = Hypergraph(edges)
        >>> graph.nvertices()
        3
        >>> graph.nedges()
        3
    """

    def __init__(self, edges: List[Hyperedge]) -> None:
        """Initialize a hypergraph with the given edges.

        Args:
            edges: List of hyperedges defining the hypergraph structure.
        """
        self._edges = edges
        self._vertex_set = set()
        self._edge_list = set(edges)
        for edge in edges:
            self._vertex_set.update(edge.vertices)

    def nedges(self) -> int:
        """Return the number of hyperedges in the hypergraph."""
        return len(self._edges)

    def nvertices(self) -> int:
        """Return the number of vertices in the hypergraph."""
        return len(self._vertex_set)

    def vertices(self) -> Iterator[int]:
        """Return an iterator over vertices in sorted order.

        Returns:
            Iterator yielding vertex indices in ascending order.
        """
        return iter(sorted(self._vertex_set))

    def edges(self, part: int = 0) -> Iterator[Hyperedge]:
        """Return an iterator over hyperedges in the hypergraph.

        Args:
            part: Partition index (reserved for subclass implementations
                that support edge partitioning for parallel updates).

        Returns:
            Iterator over all hyperedges in the hypergraph.
        """
        return iter(self._edge_list)

    def __str__(self) -> str:
        return f"Hypergraph with {self.nvertices()} vertices and {self.nedges()} edges."

    def __repr__(self) -> str:
        return f"Hypergraph({list(self._edges)})"
