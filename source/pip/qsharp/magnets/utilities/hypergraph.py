# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Hypergraph data structures for representing quantum system geometries.

This module provides classes for representing hypergraphs, which generalize
graphs by allowing edges (hyperedges) to connect any number of vertices.
Hypergraphs are useful for representing interaction terms in quantum
Hamiltonians, where multi-body interactions can involve more than two sites.
"""

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

    def __str__(self) -> str:
        return str(self.vertices)

    def __repr__(self) -> str:
        return f"Hyperedge({list(self.vertices)})"


class Hypergraph:
    """A hypergraph consisting of vertices connected by hyperedges.

    A hypergraph is a generalization of a graph where edges (hyperedges) can
    connect any number of vertices. This class serves as the base class for
    various lattice geometries used in quantum simulations.

    Attributes:
        _edge_set: Set of hyperedges in the hypergraph.
        _vertex_set: Set of all unique vertex indices in the hypergraph.

    Note:
        Edge colors are managed separately by :class:`HypergraphEdgeColoring`.
        Use :meth:`edge_coloring` to generate a coloring for this hypergraph.

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
        self._edge_set = set(edges)
        for edge in edges:
            self._vertex_set.update(edge.vertices)

    @property
    def nvertices(self) -> int:
        """Return the number of vertices in the hypergraph."""
        return len(self._vertex_set)

    def vertices(self) -> Iterator[int]:
        """Iterate over all vertex indices in the hypergraph.

        Returns:
            Iterator of vertex indices in ascending order.
        """
        return iter(sorted(self._vertex_set))

    @property
    def nedges(self) -> int:
        """Return the number of hyperedges in the hypergraph."""
        return len(self._edge_set)

    def edges(self) -> Iterator[Hyperedge]:
        """Iterate over all hyperedges in the hypergraph.

        Returns:
            Iterator of all hyperedges in the hypergraph.
        """
        return iter(self._edge_set)

    def add_edge(self, edge: Hyperedge) -> None:
        """Add a hyperedge to the hypergraph.

        Args:
            edge: The Hyperedge instance to add.
        """
        self._edge_set.add(edge)
        self._vertex_set.update(edge.vertices)

    def edge_coloring(
        self, seed: Optional[int] = 0, trials: int = 1
    ) -> "HypergraphEdgeColoring":
        """Compute a (nondeterministic) greedy edge coloring of this hypergraph.

        Args:
            seed: Optional random seed for reproducibility.
            trials: Number of randomized trials to attempt. The best coloring
                (fewest colors) is returned.

        Returns:
            A :class:`HypergraphEdgeColoring` for this hypergraph.
        """
        all_edges = sorted(self.edges(), key=lambda edge: edge.vertices)

        if not all_edges:
            return HypergraphEdgeColoring(self)

        num_trials = max(trials, 1)
        best_coloring: Optional[HypergraphEdgeColoring] = None
        least_colors: Optional[int] = None

        for trial in range(num_trials):
            trial_seed = None if seed is None else seed + trial
            rng = random.Random(trial_seed)

            edge_order = list(all_edges)
            rng.shuffle(edge_order)

            coloring = HypergraphEdgeColoring(self)
            num_colors = 0

            for edge in edge_order:
                if len(edge.vertices) == 1:
                    coloring.add_edge(edge, -1)
                    continue

                assigned = False
                for color in range(num_colors):
                    used_vertices = set().union(
                        *(
                            candidate.vertices
                            for candidate in coloring.edges_of_color(color)
                        )
                    )
                    if not any(vertex in used_vertices for vertex in edge.vertices):
                        coloring.add_edge(edge, color)
                        assigned = True
                        break

                if not assigned:
                    coloring.add_edge(edge, num_colors)
                    num_colors += 1

            if least_colors is None or coloring.ncolors < least_colors:
                least_colors = coloring.ncolors
                best_coloring = coloring

        assert best_coloring is not None
        return best_coloring

    def __str__(self) -> str:
        return f"Hypergraph with {self.nvertices} vertices and {self.nedges} edges."

    def __repr__(self) -> str:
        return f"Hypergraph({list(self._edge_set)})"


class HypergraphEdgeColoring:
    """Edge-color assignment for a :class:`Hypergraph`.

    This class stores colors separately from :class:`Hypergraph` and enforces
    the rule that multi-vertex edges sharing a color do not share any vertices.

    Conventions:

    - Colors for nontrivial edges must be nonnegative integers.
    - Single-vertex edges may use a special color (for example ``-1``).
    - Only nonnegative colors contribute to :attr:`ncolors`.

    Note:
        Colors are keyed by edge vertex tuples (``edge.vertices``), not by
        ``Hyperedge`` object identity. As a result, :meth:`color` accepts any
        ``Hyperedge`` with matching vertices, while :meth:`add_edge` still
        requires an edge instance that belongs to :attr:`hypergraph`.

    Attributes:
        hypergraph: The supporting :class:`Hypergraph` whose edges can be
            colored by this instance.
    """

    def __init__(self, hypergraph: Hypergraph) -> None:
        self.hypergraph = hypergraph
        self._colors: dict[tuple[int, ...], int] = {}  # Vertices-to-color mapping
        self._used_vertices: dict[int, set[int]] = (
            {}
        )  # Set of vertices used by each color

    @property
    def ncolors(self) -> int:
        """Return the number of distinct nonnegative colors in the coloring."""
        return len(self._used_vertices)

    def color(self, edge: Hyperedge) -> Optional[int]:
        """Return the color assigned to a specific edge.

        Args:
            edge: Hyperedge to query. Any ``Hyperedge`` with the same
                ``vertices`` tuple resolves to the same stored color.

        Returns:
            The color assigned to ``edge``, or ``None`` if the edge has not
            been added to this coloring.
        """
        if not isinstance(edge, Hyperedge):
            raise TypeError(f"edge must be Hyperedge, got {type(edge).__name__}")
        return self._colors.get(edge.vertices)

    def colors(self) -> Iterator[int]:
        """Iterate over distinct nonnegative colors present in the coloring.

        Returns:
            Iterator of distinct nonnegative color indices.
        """
        return iter(self._used_vertices.keys())

    def add_edge(self, edge: Hyperedge, color: int) -> None:
        """Add ``edge`` to this coloring with the specified ``color``.

        For multi-vertex edges, this enforces that no previously added edge
        with the same color shares a vertex with ``edge``.

        Args:
            edge: The Hyperedge instance to add. This must be an edge present
                in :attr:`hypergraph` (typically one returned by
                ``hypergraph.edges()``).
            color: Color index for the edge.

        Raises:
            TypeError: If ``edge`` is not a :class:`Hyperedge`.
            ValueError: If ``edge`` is not part of :attr:`hypergraph`.
            ValueError: If ``color`` is negative for a nontrivial edge.
            RuntimeError: If adding ``edge`` would create a same-color vertex
                conflict.
        """
        if not isinstance(edge, Hyperedge):
            raise TypeError(f"edge must be Hyperedge, got {type(edge).__name__}")

        if edge not in self.hypergraph.edges():
            raise ValueError("edge must belong to the supporting Hypergraph")

        vertices = edge.vertices

        if len(vertices) == 1:
            # Single-vertex edges can be colored with a special color (e.g., -1)
            self._colors[vertices] = color
        else:
            if color < 0:
                raise ValueError(
                    "Color index must be nonnegative for multi-vertex edges."
                )
            if color not in self._used_vertices:
                self._colors[vertices] = color
                self._used_vertices[color] = set(vertices)
            else:
                if any(v in self._used_vertices[color] for v in vertices):
                    raise RuntimeError(
                        "Edge conflicts with existing edge of same color."
                    )
                self._colors[vertices] = color
                self._used_vertices[color].update(vertices)

        self._colors[vertices] = color

    def edges_of_color(self, color: int) -> Iterator[Hyperedge]:
        """Iterate over hyperedges with a specific color.

        Args:
            color: Color index for filtering edges.

        Returns:
            Iterator of edges currently assigned to ``color``.
        """
        return iter(
            [
                edge
                for edge in self.hypergraph.edges()
                if self._colors.get(edge.vertices) == color
            ]
        )
