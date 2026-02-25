# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Complete graph geometries for quantum simulations.

This module provides classes for representing complete graphs and complete
bipartite graphs as hypergraphs. These structures are useful for quantum
systems with all-to-all or bipartite all-to-all interactions.
"""

from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    edge_coloring as default_edge_coloring,
)


class CompleteGraph(Hypergraph):
    """A complete graph where every vertex is connected to every other vertex.

    In a complete graph K_n, there are n vertices and n(n-1)/2 edges,
    with each pair of distinct vertices connected by exactly one edge.

    Attributes:
        n: Number of vertices in the graph.

    Example:

    .. code-block:: python
        >>> graph = CompleteGraph(4)
        >>> graph.nvertices
        4
        >>> graph.nedges
        6
    """

    def __init__(self, n: int, self_loops: bool = False) -> None:
        """Initialize a complete graph.

        Args:
            n: Number of vertices in the graph.
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """
        if self_loops:
            _edges = [Hyperedge([i]) for i in range(n)]
        else:
            _edges = []

        # Add all pairs of vertices
        for i in range(n):
            for j in range(i + 1, n):
                _edges.append(Hyperedge([i, j]))
        super().__init__(_edges)

        self.n = n


class CompleteBipartiteGraph(Hypergraph):
    """A complete bipartite graph with two vertex sets.

    In a complete bipartite graph K_{m,n} (m <= n), there are m + n
    vertices partitioned into two sets of sizes m and n. Every vertex
    in the first set is connected to every vertex in the second set,
    giving m * n edges total.

    Vertices 0 to m-1 form the first set, and vertices m to m+n-1
    form the second set.

    Attributes:
        m: Number of vertices in the first set.
        n: Number of vertices in the second set.

    Requires:
        m <= n

    Example:

    .. code-block:: python
        >>> graph = CompleteBipartiteGraph(2, 3)
        >>> graph.nvertices
        5
        >>> graph.nedges
        6
    """

    def __init__(self, m: int, n: int, self_loops: bool = False) -> None:
        """Initialize a complete bipartite graph.

        Args:
            m: Number of vertices in the first set (vertices 0 to m-1).
            n: Number of vertices in the second set (vertices m to m+n-1).
            self_loops: If True, include self-loop edges on each vertex
                for single-site terms.
        """
        assert m <= n, "Require m <= n for CompleteBipartiteGraph."
        total_vertices = m + n

        if self_loops:
            _edges = [Hyperedge([i]) for i in range(total_vertices)]

        else:
            _edges = []

        # Connect every vertex in first set to every vertex in second set
        for i in range(m):
            for j in range(m, m + n):
                _edges.append(Hyperedge([i, j]))
        super().__init__(_edges)

        self.m = m
        self.n = n


def edge_coloring(hypergraph: Hypergraph) -> HypergraphEdgeColoring:
    """Compute edge coloring for complete graph geometries.

    This function specializes coloring for :class:`CompleteGraph` and
    :class:`CompleteBipartiteGraph`.

    Behavior:

    - ``CompleteGraph``:
      - Self-loops (single-vertex edges) are assigned color ``-1``.
      - For even ``n``, uses an ``n-1`` color construction.
      - For odd ``n``, uses an ``n`` color construction.
    - ``CompleteBipartiteGraph``:
      - Self-loops are assigned color ``-1``.
      - Bipartite edges are assigned ``(i + j - m) % n`` where ``i`` is from
        the first partition and ``j`` is from the second.

    Args:
        hypergraph: Complete-geometry hypergraph instance to color.

    Returns:
        A :class:`HypergraphEdgeColoring` for ``hypergraph``.

    Example:

    .. code-block:: python
        >>> graph = CompleteGraph(4)
        >>> coloring = edge_coloring(graph)
        >>> sorted(coloring.colors())
        [0, 1, 2]
    """

    if isinstance(hypergraph, CompleteGraph):
        coloring = HypergraphEdgeColoring(hypergraph)
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)  # Self-loop edges get color -1
            else:
                if hypergraph.n % 2 == 0:
                    # Even case: n-1 colors needed
                    i, j = edge.vertices
                    m = hypergraph.n - 1
                    if j == m:
                        coloring.add_edge(edge, i)
                    elif (j - i) % 2 == 0:
                        coloring.add_edge(edge, (j - i) // 2)
                    else:
                        coloring.add_edge(edge, (j - i + m) // 2)
                else:
                    m = hypergraph.n
                    i, j = edge.vertices
                    if (j - i) % 2 == 0:
                        coloring.add_edge(edge, (j - i) // 2)
                    else:
                        coloring.add_edge(edge, (j - i + m) // 2)
        return coloring

    if isinstance(hypergraph, CompleteBipartiteGraph):
        coloring = HypergraphEdgeColoring(hypergraph)
        m = hypergraph.m
        n = hypergraph.n
        for edge in hypergraph.edges():
            if len(edge.vertices) == 1:
                coloring.add_edge(edge, -1)  # Self-loop edges get color -1
            else:
                i, j = edge.vertices
                coloring.add_edge(edge, (i + j - m) % n)
        return coloring

    return default_edge_coloring(hypergraph)

    # Color edges based on the second vertex index to create n parallel partitions
    # for i in range(m):
    #    for j in range(m, m + n):
    #        self.color[(i, j)] = (
    #            i + j - m
    #        ) % n  # Color edges based on second vertex index

    # Edge coloring for parallel updates
    # The even case: n-1 colors are needed
    # if n % 2 == 0:
    #    m = n - 1
    #    for i in range(m):
    #        self.color[(i, n - 1)] = (
    #            i  # Connect vertex n-1 to all others with unique colors
    #        )
    #        for j in range(1, (m - 1) // 2 + 1):
    #            a = (i + j) % m
    #            b = (i - j) % m
    #            if a < b:
    #                self.color[(a, b)] = i
    #            else:
    #                self.color[(b, a)] = i

    # The odd case: n colors are needed
    # This is the round-robin tournament scheduling algorithm for odd n
    # Set m = n for ease of reading
    # else:
    #    m = n
    #    for i in range(m):
    #        for j in range(1, (m - 1) // 2 + 1):
    #            a = (i + j) % m
    #            b = (i - j) % m
    #            if a < b:
    #                self.color[(a, b)] = i
    #            else:
    #                self.color[(b, a)] = i
