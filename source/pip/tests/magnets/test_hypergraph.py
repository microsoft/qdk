# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for hypergraph data structures."""

import pytest

from qsharp.magnets.utilities import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    greedy_edge_coloring,
)


# Hyperedge tests


def test_hyperedge_init_basic():
    """Test basic Hyperedge initialization."""
    edge = Hyperedge([0, 1])
    assert edge.vertices == (0, 1)


def test_hyperedge_vertices_sorted():
    """Test that vertices are automatically sorted."""
    edge = Hyperedge([3, 1, 2])
    assert edge.vertices == (1, 2, 3)


def test_hyperedge_single_vertex():
    """Test hyperedge with single vertex (self-loop)."""
    edge = Hyperedge([5])
    assert edge.vertices == (5,)
    assert len(edge.vertices) == 1


def test_hyperedge_multiple_vertices():
    """Test hyperedge with multiple vertices (multi-body interaction)."""
    edge = Hyperedge([0, 1, 2, 3])
    assert edge.vertices == (0, 1, 2, 3)
    assert len(edge.vertices) == 4


def test_hyperedge_repr():
    """Test string representation."""
    edge = Hyperedge([1, 0])
    assert repr(edge) == "Hyperedge([0, 1])"


def test_hyperedge_empty_vertices():
    """Test hyperedge with empty vertex list."""
    edge = Hyperedge([])
    assert edge.vertices == ()
    assert len(edge.vertices) == 0


def test_hyperedge_duplicate_vertices():
    """Test that duplicate vertices are removed."""
    edge = Hyperedge([1, 2, 2, 1, 3])
    assert edge.vertices == (1, 2, 3)


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


def test_hypergraph_edges_of_color():
    """Test HypergraphEdgeColoring returns edges with a specific color."""
    edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    coloring = HypergraphEdgeColoring(graph)
    coloring.add_edge(edges[0], 0)
    coloring.add_edge(edges[1], 0)
    edge_list = list(coloring.edges_of_color(0))
    assert len(edge_list) == 2


def test_hypergraph_edge_coloring_stores_supporting_hypergraph():
    """Test HypergraphEdgeColoring keeps a reference to its Hypergraph."""
    graph = Hypergraph([Hyperedge([0, 1])])
    coloring = HypergraphEdgeColoring(graph)
    assert coloring.hypergraph is graph


def test_hypergraph_edge_coloring_rejects_non_hyperedge():
    """Test add_edge rejects non-Hyperedge values."""
    graph = Hypergraph([Hyperedge([0, 1])])
    coloring = HypergraphEdgeColoring(graph)

    with pytest.raises(TypeError, match="edge must be Hyperedge"):
        coloring.add_edge((0, 1), 0)


def test_hypergraph_edge_coloring_rejects_edge_not_in_hypergraph():
    """Test add_edge rejects Hyperedge values not in the supporting Hypergraph."""
    graph = Hypergraph([Hyperedge([0, 1])])
    coloring = HypergraphEdgeColoring(graph)

    with pytest.raises(
        ValueError, match="edge must belong to the supporting Hypergraph"
    ):
        coloring.add_edge(Hyperedge([1, 2]), 0)


def test_hypergraph_edge_coloring_rejects_negative_color_for_nontrivial_edge():
    """Test add_edge raises ValueError for negative color on nontrivial edges."""
    edge = Hyperedge([0, 1])
    graph = Hypergraph([edge])
    coloring = HypergraphEdgeColoring(graph)

    with pytest.raises(
        ValueError, match="Color index must be nonnegative for multi-vertex edges"
    ):
        coloring.add_edge(edge, -1)


def test_hypergraph_edge_coloring_rejects_conflicting_edge():
    """Test add_edge raises RuntimeError when same-color edges share a vertex."""
    edge1 = Hyperedge([0, 1])
    edge2 = Hyperedge([1, 2])
    graph = Hypergraph([edge1, edge2])
    coloring = HypergraphEdgeColoring(graph)

    coloring.add_edge(edge1, 0)

    with pytest.raises(
        RuntimeError, match="Edge conflicts with existing edge of same color"
    ):
        coloring.add_edge(edge2, 0)


def test_hypergraph_add_edge():
    """Test adding an edge to the hypergraph."""
    graph = Hypergraph([])
    graph.add_edge(Hyperedge([0, 1]))
    assert graph.nedges == 1
    assert graph.nvertices == 2


def test_hypergraph_add_edge_with_color():
    """Test assigning colors via HypergraphEdgeColoring."""
    graph = Hypergraph([Hyperedge([0, 1])])
    edge = Hyperedge([2, 3])
    graph.add_edge(edge)
    coloring = HypergraphEdgeColoring(graph)
    coloring.add_edge(edge, color=1)
    assert graph.nedges == 2
    assert coloring.color(edge) == 1


def test_hypergraph_color_default():
    """Test that Hypergraph has no built-in color mapping."""
    graph = Hypergraph([Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])])
    assert not hasattr(graph, "color")


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
    colored = greedy_edge_coloring(graph)
    assert isinstance(colored, HypergraphEdgeColoring)
    assert len(list(colored.colors())) == 0
    assert colored.ncolors == 0


def test_greedy_edge_coloring_single_edge():
    """Test greedy edge coloring with a single edge."""
    edge = Hyperedge([0, 1])
    graph = Hypergraph([edge])
    colored = greedy_edge_coloring(graph, seed=42)
    assert colored.color(edge) == 0
    assert colored.ncolors == 1


def test_greedy_edge_coloring_non_overlapping():
    """Test coloring of non-overlapping edges (can share color)."""
    edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)
    # Non-overlapping edges can be in the same color
    assert colored.color(edges[0]) is not None
    assert colored.color(edges[1]) is not None
    assert colored.ncolors == 1


def test_greedy_edge_coloring_overlapping():
    """Test coloring of overlapping edges (need different colors)."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)
    # Overlapping edges need different colors
    assert colored.color(edges[0]) is not None
    assert colored.color(edges[1]) is not None
    assert colored.ncolors == 2


def test_greedy_edge_coloring_triangle():
    """Test coloring of a triangle (3 edges, all pairwise overlapping)."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)
    # All edges share vertices pairwise, so need 3 colors
    assert colored.color(edges[0]) is not None
    assert colored.color(edges[1]) is not None
    assert colored.color(edges[2]) is not None
    assert colored.ncolors == 3


def test_greedy_edge_coloring_validity():
    """Test that coloring is valid (no two edges with same color share a vertex)."""
    edges = [
        Hyperedge([0, 1]),
        Hyperedge([1, 2]),
        Hyperedge([2, 3]),
        Hyperedge([3, 4]),
        Hyperedge([0, 4]),
    ]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)

    # Group edges by color
    colors = {}
    for edge in edges:
        color = colored.color(edge)
        assert color is not None
        if color not in colors:
            colors[color] = []
        colors[color].append(edge.vertices)

    # Verify each color group has no overlapping edges
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            # No vertex should already be used in this color
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_greedy_edge_coloring_all_edges_colored():
    """Test that all edges are assigned a color."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)

    # All edges should have a color assigned
    assert colored.color(edges[0]) is not None
    assert colored.color(edges[1]) is not None
    assert colored.color(edges[2]) is not None


def test_greedy_edge_coloring_reproducible_with_seed():
    """Test that coloring is reproducible with the same seed."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3]), Hyperedge([0, 3])]
    graph = Hypergraph(edges)

    colored1 = greedy_edge_coloring(graph, seed=123)
    colored2 = greedy_edge_coloring(graph, seed=123)

    color_map_1 = {edge.vertices: colored1.color(edge) for edge in edges}
    color_map_2 = {edge.vertices: colored2.color(edge) for edge in edges}
    assert color_map_1 == color_map_2


def test_greedy_edge_coloring_multiple_trials():
    """Test that multiple trials can find better colorings."""
    edges = [
        Hyperedge([0, 1]),
        Hyperedge([1, 2]),
        Hyperedge([2, 3]),
        Hyperedge([3, 0]),
    ]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42, trials=10)
    # A cycle of 4 edges can be 2-colored
    assert colored.ncolors <= 3  # Greedy may not always find optimal


def test_greedy_edge_coloring_hyperedges():
    """Test coloring with multi-vertex hyperedges."""
    edges = [
        Hyperedge([0, 1, 2]),
        Hyperedge([2, 3, 4]),
        Hyperedge([5, 6, 7]),
    ]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)

    # First two share vertex 2, third is independent
    assert colored.color(edges[0]) is not None
    assert colored.color(edges[1]) is not None
    assert colored.color(edges[2]) is not None
    assert colored.ncolors >= 2


def test_greedy_edge_coloring_self_loops():
    """Test coloring with self-loop edges."""
    edges = [Hyperedge([0]), Hyperedge([1]), Hyperedge([2])]
    graph = Hypergraph(edges)
    colored = greedy_edge_coloring(graph, seed=42)

    # Self-loops use the special -1 color and do not contribute to ncolors.
    assert colored.color(edges[0]) == -1
    assert colored.color(edges[1]) == -1
    assert colored.color(edges[2]) == -1
    assert colored.ncolors == 0
