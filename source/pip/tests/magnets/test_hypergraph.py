# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for hypergraph data structures."""

from qsharp.magnets.geometry.hypergraph import Hyperedge, Hypergraph, greedyEdgeColoring


# Hyperedge tests


def test_hyperedge_init_basic():
    """Test basic Hyperedge initialization."""
    edge = Hyperedge([0, 1])
    assert edge.vertices == [0, 1]


def test_hyperedge_vertices_sorted():
    """Test that vertices are automatically sorted."""
    edge = Hyperedge([3, 1, 2])
    assert edge.vertices == [1, 2, 3]


def test_hyperedge_single_vertex():
    """Test hyperedge with single vertex (self-loop)."""
    edge = Hyperedge([5])
    assert edge.vertices == [5]
    assert len(edge.vertices) == 1


def test_hyperedge_multiple_vertices():
    """Test hyperedge with multiple vertices (multi-body interaction)."""
    edge = Hyperedge([0, 1, 2, 3])
    assert edge.vertices == [0, 1, 2, 3]
    assert len(edge.vertices) == 4


def test_hyperedge_repr():
    """Test string representation."""
    edge = Hyperedge([1, 0])
    assert repr(edge) == "Hyperedge([0, 1])"


def test_hyperedge_empty_vertices():
    """Test hyperedge with empty vertex list."""
    edge = Hyperedge([])
    assert edge.vertices == []
    assert len(edge.vertices) == 0


def test_hyperedge_duplicate_vertices():
    """Test that duplicate vertices are removed."""
    edge = Hyperedge([1, 2, 2, 1, 3])
    assert edge.vertices == [1, 2, 3]


# Hypergraph tests


def test_hypergraph_init_basic():
    """Test basic Hypergraph initialization."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    assert graph.nedges == 2
    assert graph.nvertices == 3


def test_hypergraph_empty_graph():
    """Test hypergraph with no edges."""
    graph = Hypergraph([])
    assert graph.nedges == 0
    assert graph.nvertices == 0


def test_hypergraph_nedges():
    """Test edge count."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    assert graph.nedges == 3


def test_hypergraph_nvertices():
    """Test vertex count with unique vertices."""
    edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    assert graph.nvertices == 4


def test_hypergraph_nvertices_with_shared_vertices():
    """Test vertex count when edges share vertices."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
    graph = Hypergraph(edges)
    assert graph.nvertices == 3


def test_hypergraph_vertices_iterator():
    """Test vertices iterator returns sorted vertices."""
    edges = [Hyperedge([3, 1]), Hyperedge([0, 2])]
    graph = Hypergraph(edges)
    vertices = list(graph.vertices())
    assert vertices == [0, 1, 2, 3]


def test_hypergraph_edges_iterator():
    """Test edges iterator returns all edges."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    edge_list = list(graph.edges())
    assert len(edge_list) == 2


def test_hypergraph_edges_by_part():
    """Test edgesByPart returns edges in a specific partition."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    # Default: all edges in part 0
    edge_list = list(graph.edgesByPart(0))
    assert len(edge_list) == 2


def test_hypergraph_add_edge():
    """Test adding an edge to the hypergraph."""
    graph = Hypergraph([])
    graph.addEdge(Hyperedge([0, 1]))
    assert graph.nedges == 1
    assert graph.nvertices == 2


def test_hypergraph_add_edge_to_part():
    """Test adding edges to different partitions."""
    graph = Hypergraph([Hyperedge([0, 1])])
    graph.parts.append([])  # Add a second partition
    graph.addEdge(Hyperedge([2, 3]), part=1)
    assert graph.nedges == 2
    assert len(graph.parts[0]) == 1
    assert len(graph.parts[1]) == 1


def test_hypergraph_parts_default():
    """Test that default parts contain all edge indices."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    assert len(graph.parts) == 1
    assert graph.parts[0] == [0, 1, 2]


def test_hypergraph_str():
    """Test string representation."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    expected = "Hypergraph with 4 vertices and 3 edges."
    assert str(graph) == expected


def test_hypergraph_repr():
    """Test repr representation."""
    edges = [Hyperedge([0, 1])]
    graph = Hypergraph(edges)
    result = repr(graph)
    assert "Hypergraph" in result
    assert "Hyperedge" in result


def test_hypergraph_single_vertex_edges():
    """Test hypergraph with self-loop edges."""
    edges = [Hyperedge([0]), Hyperedge([1]), Hyperedge([2])]
    graph = Hypergraph(edges)
    assert graph.nedges == 3
    assert graph.nvertices == 3


def test_hypergraph_mixed_edge_sizes():
    """Test hypergraph with edges of different sizes."""
    edges = [
        Hyperedge([0]),  # 1 vertex (self-loop)
        Hyperedge([1, 2]),  # 2 vertices (pair)
        Hyperedge([3, 4, 5]),  # 3 vertices (triple)
    ]
    graph = Hypergraph(edges)
    assert graph.nedges == 3
    assert graph.nvertices == 6


def test_hypergraph_non_contiguous_vertices():
    """Test hypergraph with non-contiguous vertex indices."""
    edges = [Hyperedge([0, 10]), Hyperedge([5, 20])]
    graph = Hypergraph(edges)
    assert graph.nvertices == 4
    vertices = list(graph.vertices())
    assert vertices == [0, 5, 10, 20]


# greedyEdgeColoring tests


def test_greedy_edge_coloring_empty():
    """Test greedy edge coloring on empty hypergraph."""
    graph = Hypergraph([])
    colored = greedyEdgeColoring(graph)
    assert colored.nedges == 0
    assert len(colored.parts) == 1
    assert colored.parts[0] == []


def test_greedy_edge_coloring_single_edge():
    """Test greedy edge coloring with a single edge."""
    graph = Hypergraph([Hyperedge([0, 1])])
    colored = greedyEdgeColoring(graph, seed=42)
    assert colored.nedges == 1
    assert len(colored.parts) == 1


def test_greedy_edge_coloring_non_overlapping():
    """Test coloring of non-overlapping edges (can share color)."""
    edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)
    # Non-overlapping edges can be in the same color
    assert colored.nedges == 2
    assert len(colored.parts) == 1


def test_greedy_edge_coloring_overlapping():
    """Test coloring of overlapping edges (need different colors)."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)
    # Overlapping edges need different colors
    assert colored.nedges == 2
    assert len(colored.parts) == 2


def test_greedy_edge_coloring_triangle():
    """Test coloring of a triangle (3 edges, all pairwise overlapping)."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)
    # All edges share vertices pairwise, so need 3 colors
    assert colored.nedges == 3
    assert len(colored.parts) == 3


def test_greedy_edge_coloring_validity():
    """Test that coloring is valid (no two edges in same part share a vertex)."""
    edges = [
        Hyperedge([0, 1]),
        Hyperedge([1, 2]),
        Hyperedge([2, 3]),
        Hyperedge([3, 4]),
        Hyperedge([0, 4]),
    ]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)

    # Verify each part has no overlapping edges
    for part in colored.parts:
        used_vertices = set()
        for edge_idx in part:
            edge = colored._edge_list[edge_idx]
            # No vertex should already be used in this part
            assert not any(v in used_vertices for v in edge.vertices)
            used_vertices.update(edge.vertices)


def test_greedy_edge_coloring_all_edges_colored():
    """Test that all edges are assigned to exactly one part."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)

    # Collect all edge indices from all parts
    all_colored = []
    for part in colored.parts:
        all_colored.extend(part)

    # Should have exactly 3 edges colored, each once
    assert sorted(all_colored) == [0, 1, 2]


def test_greedy_edge_coloring_reproducible_with_seed():
    """Test that coloring is reproducible with the same seed."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3]), Hyperedge([0, 3])]
    graph = Hypergraph(edges)

    colored1 = greedyEdgeColoring(graph, seed=123)
    colored2 = greedyEdgeColoring(graph, seed=123)

    assert colored1.parts == colored2.parts


def test_greedy_edge_coloring_multiple_trials():
    """Test that multiple trials can find better colorings."""
    edges = [
        Hyperedge([0, 1]),
        Hyperedge([1, 2]),
        Hyperedge([2, 3]),
        Hyperedge([3, 0]),
    ]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42, trials=10)
    # A cycle of 4 edges can be 2-colored
    assert len(colored.parts) <= 3  # Greedy may not always find optimal


def test_greedy_edge_coloring_hyperedges():
    """Test coloring with multi-vertex hyperedges."""
    edges = [
        Hyperedge([0, 1, 2]),
        Hyperedge([2, 3, 4]),
        Hyperedge([5, 6, 7]),
    ]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)

    # First two share vertex 2, third is independent
    assert colored.nedges == 3
    assert len(colored.parts) >= 2


def test_greedy_edge_coloring_self_loops():
    """Test coloring with self-loop edges."""
    edges = [Hyperedge([0]), Hyperedge([1]), Hyperedge([2])]
    graph = Hypergraph(edges)
    colored = greedyEdgeColoring(graph, seed=42)

    # Self-loops don't share vertices, can all be same color
    assert colored.nedges == 3
    assert len(colored.parts) == 1
