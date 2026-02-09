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
    stored as a sorted tuple for consistency and hashability.

    Attributes:
        vertices: Sorted tuple of vertex indices connected by this hyperedge.

    Example:

    .. code-block:: python
        >>> edge = Hyperedge([2, 0, 1])
        >>> edge.vertices
        (0, 1, 2)
    """

    def __init__(self, vertices: list[int]) -> None:
        """Initialize a hyperedge with the given vertices.

        Args:
            vertices: List of vertex indices. Will be sorted internally.
        """
        self.vertices: tuple[int, ...] = tuple(sorted(set(vertices)))

    def __repr__(self) -> str:
        return f"Hyperedge({list(self.vertices)})"


class Hypergraph:
    """A hypergraph consisting of vertices connected by hyperedges.

    A hypergraph is a generalization of a graph where edges (hyperedges) can
    connect any number of vertices. This class serves as the base class for
    various lattice geometries used in quantum simulations.

    Attributes:
        _edge_list: List of hyperedges in the order they were added.
        _vertex_set: Set of all unique vertex indices in the hypergraph.
        color: Dictionary mapping edge vertex tuples to color indices. Initially
            all edges have color index 0. This is useful for parallelism in
            certain architectures.

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
        self.color: dict[tuple[int, ...], int] = {}  # All edges start with color 0
        for edge in edges:
            self._vertex_set.update(edge.vertices)
            self.color[edge.vertices] = 0

    @property
    def ncolors(self) -> int:
        """Return the number of distinct colors used in the edge coloring."""
        return len(set(self.color.values()))

    @property
    def nedges(self) -> int:
        """Return the number of hyperedges in the hypergraph."""
        return len(self._edge_list)

    @property
    def nvertices(self) -> int:
        """Return the number of vertices in the hypergraph."""
        return len(self._vertex_set)

    def add_edge(self, edge: Hyperedge, color: int = 0) -> None:
        """Add a hyperedge to the hypergraph.

        Args:
            edge: The Hyperedge instance to add.
            color: Color index for the edge, used for implementations
                with edge coloring for parallel updates. By
                default, all edges are assigned color 0.
        """
        self._edge_list.append(edge)
        self._vertex_set.update(edge.vertices)
        self.color[edge.vertices] = color

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

    def edges_by_color(self, color: int) -> Iterator[Hyperedge]:
        """Iterate over hyperedges with a specific color.

        Args:
            color: Color index for filtering edges.

        Returns:
            Iterator of hyperedges with the specified color.
        """
        return iter(
            [edge for edge in self._edge_list if self.color[edge.vertices] == color]
        )

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

    used_vertices: list[set[int]] = [set()]  # Vertices used by each color
    num_colors = 1

    for i in range(len(edge_indexes)):
        edge = hypergraph._edge_list[edge_indexes[i]]
        for j in range(num_colors + 1):

            # If we've reached a new color, add it
            if j == num_colors:
                used_vertices.append(set())
                num_colors += 1

            # Check if this edge can be added to color j
            # Note that we always match on the last color if it was added
            # if so, add it and break
            if not any(v in used_vertices[j] for v in edge.vertices):
                best.color[edge.vertices] = j
                used_vertices[j].update(edge.vertices)
                break

    least_colors = num_colors

    # To do: parallelize over trials
    for trial in range(1, trials):

        # Set random seed for reproducibility
        # Designed to work with parallel trials
        if seed is not None:
            random.seed(seed + trial)

        # Shuffle edge indices to randomize insertion order
        edge_indexes = list(range(hypergraph.nedges))
        random.shuffle(edge_indexes)

        edge_colors: dict[tuple[int, ...], int] = {}  # Edge to color mapping
        used_vertices = [set()]  # Vertices used by each color
        num_colors = 1

        for i in range(len(edge_indexes)):
            edge = hypergraph._edge_list[edge_indexes[i]]
            for j in range(num_colors + 1):

                # If we've reached a new color, add it
                if j == num_colors:
                    used_vertices.append(set())
                    num_colors += 1

                # Check if this edge can be added to color j
                # if so, add it and break
                if not any(v in used_vertices[j] for v in edge.vertices):
                    edge_colors[edge.vertices] = j
                    used_vertices[j].update(edge.vertices)
                    break

        # If this trial used fewer colors, update best
        if num_colors < least_colors:
            least_colors = num_colors
            best.color = deepcopy(edge_colors)

    return best
