# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Complete graph geometries for quantum simulations.

This module provides classes for representing complete graphs and complete
bipartite graphs as hypergraphs. These structures are useful for quantum
systems with all-to-all or bipartite all-to-all interactions.
"""

from qsharp.magnets.geometry.hypergraph import (
    Hyperedge,
    Hypergraph,
    greedy_edge_coloring,
)


class CompleteGraph(Hypergraph):
    """A complete graph where every vertex is connected to every other vertex.

    In a complete graph K_n, there are n vertices and n(n-1)/2 edges,
    with each pair of distinct vertices connected by exactly one edge.

    To do: edge partitioning for parallel updates.

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

        # Set colors for self-loop edges if enabled
        if self_loops:
            for i in range(n):
                self.color[(i,)] = -1  # Self-loop edges get color -1

        # Edge coloring for parallel updates
        # The even case: n-1 colors are needed
        if n % 2 == 0:
            m = n - 1
            for i in range(m):
                self.color[(i, n - 1)] = (
                    i  # Connect vertex n-1 to all others with unique colors
                )
                for j in range(1, (m - 1) // 2 + 1):
                    a = (i + j) % m
                    b = (i - j) % m
                    if a < b:
                        self.color[(a, b)] = i
                    else:
                        self.color[(b, a)] = i

        # The odd case: n colors are needed
        # This is the round-robin tournament scheduling algorithm for odd n
        # Set m = n for ease of reading
        else:
            m = n
            for i in range(m):
                for j in range(1, (m - 1) // 2 + 1):
                    a = (i + j) % m
                    b = (i - j) % m
                    if a < b:
                        self.color[(a, b)] = i
                    else:
                        self.color[(b, a)] = i

        self.n = n


class CompleteBipartiteGraph(Hypergraph):
    """A complete bipartite graph with two vertex sets.

    In a complete bipartite graph K_{m,n} (m <= n), there are m + n
    vertices partitioned into two sets of sizes m and n. Every vertex
    in the first set is connected to every vertex in the second set,
    giving m * n edges total.

    Vertices 0 to m-1 form the first set, and vertices m to m+n-1
    form the second set.

    To do: edge partitioning for parallel updates.

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
                edge_idx = len(_edges)
                _edges.append(Hyperedge([i, j]))
        super().__init__(_edges)

        # Set colors for self-loop edges if enabled
        if self_loops:
            for i in range(total_vertices):
                self.color[(i,)] = -1  # Self-loop edges get color -1

        # Color edges based on the second vertex index to create n parallel partitions
        for i in range(m):
            for j in range(m, m + n):
                self.color[(i, j)] = (
                    i + j - m
                ) % n  # Color edges based on second vertex index

        self.m = m
        self.n = n
