# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Hypergraph data structures for representing quantum system geometries.

This module provides classes for representing hypergraphs, which generalize
graphs by allowing edges (hyperedges) to connect any number of vertices.
Hypergraphs are useful for representing interaction terms in quantum
Hamiltonians, where multi-body interactions can involve more than two sites.
"""

from copy import deepcopy
import random
from typing import Iterator, Optional


class Hyperedge:
    """A hyperedge connecting one or more vertices in a hypergraph.

    A hyperedge generalizes the concept of an edge in a graph. While a
    traditional edge connects exactly two vertices, a hyperedge can connect
    any number of vertices. This is useful for representing:
    - Single-site terms (self-loops): 1 vertex
    - Two-body interactions: 2 vertices
    - Multi-body interactions: 3+ vertices
    Each hyperedge is defined by a set of unique vertex indices, which are
    stored in sorted order for consistency.

    Attributes:
        vertices: Sorted list of vertex indices connected by this hyperedge.

    Example:

    .. code-block:: python
        >>> edge = Hyperedge([2, 0, 1])
        >>> edge.vertices
        [0, 1, 2]
    """

    def __init__(self, vertices: list[int]) -> None:
        """Initialize a hyperedge with the given vertices.

        Args:
            vertices: List of vertex indices. Will be sorted internally.
        """
        self.vertices: list[int] = sorted(set(vertices))

    def __repr__(self) -> str:
        return f"Hyperedge({self.vertices})"


class Hypergraph:
    """A hypergraph consisting of vertices connected by hyperedges.

    A hypergraph is a generalization of a graph where edges (hyperedges) can
    connect any number of vertices. This class serves as the base class for
    various lattice geometries used in quantum simulations.

    Attributes:
        _edge_list: List of hyperedges in the order they were added.
        _vertex_set: Set of all unique vertex indices in the hypergraph.
        parts: List of lists, where each sublist contains indices of edges
            belonging to a specific part of an edge partitioning. This is useful
            for parallelism in certain architectures.

    Example:

    .. code-block:: python
        >>> edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
        >>> graph = Hypergraph(edges)
        >>> graph.nvertices
        3
        >>> graph.nedges
        3
    """

    def __init__(self, edges: list[Hyperedge]) -> None:
        """Initialize a hypergraph with the given edges.

        Args:
            edges: List of hyperedges defining the hypergraph structure.
        """
        self._vertex_set = set()
        self._edge_list = edges
        self.parts = [list(range(len(edges)))]  # Single partition by default
        for edge in edges:
            self._vertex_set.update(edge.vertices)

    @property
    def nedges(self) -> int:
        """Return the number of hyperedges in the hypergraph."""
        return len(self._edge_list)

    @property
    def nvertices(self) -> int:
        """Return the number of vertices in the hypergraph."""
        return len(self._vertex_set)

    def add_edge(self, edge: Hyperedge, part: int = 0) -> None:
        """Add a hyperedge to the hypergraph.

        Args:
            edge: The Hyperedge instance to add.
            part: Partition index, used for implementations
                with edge partitioning for parallel updates. By
                default, all edges are added to the single part
                with index 0.
        """
        self._edge_list.append(edge)
        self._vertex_set.update(edge.vertices)
        self.parts[part].append(len(self._edge_list) - 1)  # Add to specified partition

    def vertices(self) -> Iterator[int]:
        """Iterate over all vertex indices in the hypergraph.

        Returns:
            Iterator of vertex indices in ascending order.
        """
        return iter(sorted(self._vertex_set))

    def edges(self) -> Iterator[Hyperedge]:
        """Iterate over all hyperedges in the hypergraph.

        Returns:
            Iterator of all hyperedges in the hypergraph.
        """
        return iter(self._edge_list)

    def edges_by_part(self, part: int) -> Iterator[Hyperedge]:
        """Iterate over hyperedges in a specific partition of the hypergraph.

        Args:
            part: Partition index, used for implementations
                with edge partitioning for parallel updates. By
                default, all edges are in a single part with
                index 0.

        Returns:
            Iterator of hyperedges in the specified partition.
        """
        return iter([self._edge_list[i] for i in self.parts[part]])

    def __str__(self) -> str:
        return f"Hypergraph with {self.nvertices} vertices and {self.nedges} edges."

    def __repr__(self) -> str:
        return f"Hypergraph({list(self._edge_list)})"


def greedy_edge_coloring(
    hypergraph: Hypergraph,  # The hypergraph to color.
    seed: Optional[int] = None,  # Random seed for reproducibility.
    trials: int = 1,  # Number of trials to perform.
) -> Hypergraph:
    """Perform a (nondeterministic) greedy edge coloring of the hypergraph.
    Args:
        hypergraph: The Hypergraph instance to color.
        seed: Optional random seed for reproducibility.
        trials: Number of trials to perform. The coloring with the fewest colors
            will be returned. Default is 1.

    Returns:
        A Hypergraph where each (hyper)edge is assigned a color
        such that no two (hyper)edges sharing a vertex have the
        same color.
    """

    best = Hypergraph(hypergraph._edge_list)  # Placeholder for best coloring found

    if seed is not None:
        random.seed(seed)

    # Shuffle edge indices to randomize insertion order
    edge_indexes = list(range(hypergraph.nedges))
    random.shuffle(edge_indexes)

    best.parts = [[]]  # Initialize with one empty color part
    used_vertices = [set()]  # Vertices used by each color

    for i in range(len(edge_indexes)):
        edge = hypergraph._edge_list[edge_indexes[i]]
        for j in range(len(best.parts) + 1):

            # If we've reached a new color, add it
            if j == len(best.parts):
                best.parts.append([])
                used_vertices.append(set())

            # Check if this edge can be added to color j
            # Note that we always match on the last color if it was added
            # if so, add it and break
            if not any(v in used_vertices[j] for v in edge.vertices):
                best.parts[j].append(edge_indexes[i])
                used_vertices[j].update(edge.vertices)
                break

    least_colors = len(best.parts)

    # To do: parallelize over trials
    for trial in range(1, trials):

        # Set random seed for reproducibility
        # Designed to work with parallel trials
        if seed is not None:
            random.seed(seed + trial)

        # Shuffle edge indices to randomize insertion order
        edge_indexes = list(range(hypergraph.nedges))
        random.shuffle(edge_indexes)

        parts = [[]]  # Initialize with one empty color part
        used_vertices = [set()]  # Vertices used by each color

        for i in range(len(edge_indexes)):
            edge = hypergraph._edge_list[edge_indexes[i]]
            for j in range(len(parts) + 1):

                # If we've reached a new color, add it
                if j == len(parts):
                    parts.append([])
                    used_vertices.append(set())

                # Check if this edge can be added to color j
                # if so, add it and break
                if not any(v in used_vertices[j] for v in edge.vertices):
                    parts[j].append(edge_indexes[i])
                    used_vertices[j].update(edge.vertices)
                    break

        # If this trial used fewer colors, update best
        if len(parts) < least_colors:
            least_colors = len(parts)
            best.parts = deepcopy(parts)

    return best
