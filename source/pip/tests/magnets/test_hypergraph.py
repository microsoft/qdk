# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for hypergraph data structures."""

from qsharp.magnets.geometry.hypergraph import Hyperedge, Hypergraph


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


def test_hypergraph_edges_with_part_parameter():
    """Test edges iterator with part parameter (base class ignores it)."""
    edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
    graph = Hypergraph(edges)
    # Base class returns all edges regardless of part parameter
    edge_list_0 = list(graph.edges(part=0))
    edge_list_1 = list(graph.edges(part=1))
    assert len(edge_list_0) == 2
    assert len(edge_list_1) == 2


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
