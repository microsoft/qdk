# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for complete graph data structures."""

from qsharp.magnets.geometry.complete import CompleteBipartiteGraph, CompleteGraph


# CompleteGraph tests


def test_complete_graph_init_basic():
    """Test basic CompleteGraph initialization."""
    graph = CompleteGraph(4)
    assert graph.nvertices == 4
    assert graph.nedges == 6  # 4 * 3 / 2 = 6
    assert graph.n == 4


def test_complete_graph_single_vertex():
    """Test CompleteGraph with a single vertex (no edges)."""
    graph = CompleteGraph(1)
    assert graph.nvertices == 0
    assert graph.nedges == 0
    assert graph.n == 1


def test_complete_graph_two_vertices():
    """Test CompleteGraph with two vertices (one edge)."""
    graph = CompleteGraph(2)
    assert graph.nvertices == 2
    assert graph.nedges == 1


def test_complete_graph_three_vertices():
    """Test CompleteGraph with three vertices (triangle)."""
    graph = CompleteGraph(3)
    assert graph.nvertices == 3
    assert graph.nedges == 3


def test_complete_graph_five_vertices():
    """Test CompleteGraph with five vertices."""
    graph = CompleteGraph(5)
    assert graph.nvertices == 5
    assert graph.nedges == 10  # 5 * 4 / 2 = 10


def test_complete_graph_edges():
    """Test that CompleteGraph creates correct edges."""
    graph = CompleteGraph(4)
    edges = list(graph.edges())
    assert len(edges) == 6
    # All pairs should be present
    edge_sets = [set(e.vertices) for e in edges]
    assert {0, 1} in edge_sets
    assert {0, 2} in edge_sets
    assert {0, 3} in edge_sets
    assert {1, 2} in edge_sets
    assert {1, 3} in edge_sets
    assert {2, 3} in edge_sets


def test_complete_graph_vertices():
    """Test that CompleteGraph vertices are correct."""
    graph = CompleteGraph(5)
    vertices = list(graph.vertices())
    assert vertices == [0, 1, 2, 3, 4]


def test_complete_graph_with_self_loops():
    """Test CompleteGraph with self-loops enabled."""
    graph = CompleteGraph(4, self_loops=True)
    assert graph.nvertices == 4
    # 4 self-loops + 6 edges = 10
    assert graph.nedges == 10


def test_complete_graph_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    graph = CompleteGraph(3, self_loops=True)
    edges = list(graph.edges())
    # First 3 edges should be self-loops
    assert edges[0].vertices == (0,)
    assert edges[1].vertices == (1,)
    assert edges[2].vertices == (2,)


def test_complete_graph_edge_count_formula():
    """Test that edge count follows n(n-1)/2 formula."""
    for n in range(1, 10):
        graph = CompleteGraph(n)
        expected_edges = n * (n - 1) // 2
        assert graph.nedges == expected_edges


def test_complete_graph_str():
    """Test string representation."""
    graph = CompleteGraph(4)
    assert "4 vertices" in str(graph)
    assert "6 edges" in str(graph)


def test_complete_graph_inherits_hypergraph():
    """Test that CompleteGraph is a Hypergraph subclass with all methods."""
    from qsharp.magnets.geometry.hypergraph import Hypergraph

    graph = CompleteGraph(4)
    assert isinstance(graph, Hypergraph)
    assert hasattr(graph, "edges")
    assert hasattr(graph, "vertices")


# CompleteBipartiteGraph tests


def test_complete_bipartite_graph_init_basic():
    """Test basic CompleteBipartiteGraph initialization."""
    graph = CompleteBipartiteGraph(2, 3)
    assert graph.nvertices == 5
    assert graph.nedges == 6  # 2 * 3 = 6
    assert graph.m == 2
    assert graph.n == 3


def test_complete_bipartite_graph_single_each():
    """Test CompleteBipartiteGraph with one vertex in each set."""
    graph = CompleteBipartiteGraph(1, 1)
    assert graph.nvertices == 2
    assert graph.nedges == 1


def test_complete_bipartite_graph_one_and_many():
    """Test CompleteBipartiteGraph with one vertex in first set."""
    graph = CompleteBipartiteGraph(1, 5)
    assert graph.nvertices == 6
    assert graph.nedges == 5  # 1 * 5 = 5


def test_complete_bipartite_graph_square():
    """Test CompleteBipartiteGraph with equal set sizes."""
    graph = CompleteBipartiteGraph(3, 3)
    assert graph.nvertices == 6
    assert graph.nedges == 9  # 3 * 3 = 9


def test_complete_bipartite_graph_edges():
    """Test that CompleteBipartiteGraph creates correct edges."""
    graph = CompleteBipartiteGraph(2, 3)
    edges = list(graph.edges())
    assert len(edges) == 6
    # Vertices 0, 1 in first set; 2, 3, 4 in second set
    edge_sets = [set(e.vertices) for e in edges]
    # All pairs between sets should be present
    assert {0, 2} in edge_sets
    assert {0, 3} in edge_sets
    assert {0, 4} in edge_sets
    assert {1, 2} in edge_sets
    assert {1, 3} in edge_sets
    assert {1, 4} in edge_sets
    # No edges within sets
    assert {0, 1} not in edge_sets
    assert {2, 3} not in edge_sets
    assert {2, 4} not in edge_sets
    assert {3, 4} not in edge_sets


def test_complete_bipartite_graph_vertices():
    """Test that CompleteBipartiteGraph vertices are correct."""
    graph = CompleteBipartiteGraph(2, 3)
    vertices = list(graph.vertices())
    assert vertices == [0, 1, 2, 3, 4]


def test_complete_bipartite_graph_with_self_loops():
    """Test CompleteBipartiteGraph with self-loops enabled."""
    graph = CompleteBipartiteGraph(2, 3, self_loops=True)
    assert graph.nvertices == 5
    # 5 self-loops + 6 edges = 11
    assert graph.nedges == 11


def test_complete_bipartite_graph_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    graph = CompleteBipartiteGraph(2, 2, self_loops=True)
    edges = list(graph.edges())
    # First 4 edges should be self-loops
    assert edges[0].vertices == (0,)
    assert edges[1].vertices == (1,)
    assert edges[2].vertices == (2,)
    assert edges[3].vertices == (3,)


def test_complete_bipartite_graph_edge_count_formula():
    """Test that edge count follows m * n formula."""
    for m in range(1, 6):
        for n in range(m, 6):
            graph = CompleteBipartiteGraph(m, n)
            expected_edges = m * n
            assert graph.nedges == expected_edges


def test_complete_bipartite_graph_coloring_without_self_loops():
    """Test edge coloring without self-loops."""
    graph = CompleteBipartiteGraph(3, 4)
    # Should have n colors for bipartite coloring
    assert graph.ncolors == 4


def test_complete_bipartite_graph_coloring_with_self_loops():
    """Test edge coloring with self-loops."""
    graph = CompleteBipartiteGraph(3, 4, self_loops=True)
    # Self-loops get color -1, bipartite edges get n colors (0 to n-1)
    # So total distinct colors = n + 1 (including -1)
    assert graph.ncolors == 5


def test_complete_bipartite_graph_coloring_non_overlapping():
    """Test that edges with the same color don't share vertices."""
    graph = CompleteBipartiteGraph(3, 4)
    # Group edges by color
    colors = {}
    for edge_vertices, color in graph.color.items():
        if color not in colors:
            colors[color] = []
        colors[color].append(edge_vertices)
    # Check each color group
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_complete_bipartite_graph_str():
    """Test string representation."""
    graph = CompleteBipartiteGraph(2, 3)
    assert "5 vertices" in str(graph)
    assert "6 edges" in str(graph)


def test_complete_bipartite_graph_inherits_hypergraph():
    """Test that CompleteBipartiteGraph is a Hypergraph subclass with all methods."""
    from qsharp.magnets.geometry.hypergraph import Hypergraph

    graph = CompleteBipartiteGraph(2, 3)
    assert isinstance(graph, Hypergraph)
    assert hasattr(graph, "edges")
    assert hasattr(graph, "vertices")
    assert hasattr(graph, "edges_by_color")
