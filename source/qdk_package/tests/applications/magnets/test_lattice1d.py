# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for 1D lattice data structures."""

import pytest

cirq = pytest.importorskip("cirq")

from qdk.applications.magnets import (
    Chain1D,
    Hypergraph,
    HypergraphEdgeColoring,
    MthNearestNeighborChain1D,
    Ring1D,
)


def _vertex_color_map(graph) -> dict[tuple[int, ...], int | None]:
    coloring = graph.edge_coloring()
    return {edge.vertices: coloring.color(edge.vertices) for edge in graph.edges()}


# Chain1D tests


def test_chain1d_init_basic():
    """Test basic Chain1D initialization."""
    chain = Chain1D(4)
    assert chain.nvertices == 4
    assert chain.nedges == 3
    assert chain.length == 4


def test_chain1d_single_vertex():
    """Test Chain1D with a single vertex (no edges)."""
    chain = Chain1D(1)
    assert chain.nvertices == 0
    assert chain.nedges == 0
    assert chain.length == 1


def test_chain1d_two_vertices():
    """Test Chain1D with two vertices (one edge)."""
    chain = Chain1D(2)
    assert chain.nvertices == 2
    assert chain.nedges == 1


def test_chain1d_edges():
    """Test that Chain1D creates correct nearest-neighbor edges."""
    chain = Chain1D(4)
    edge_vertices = {edge.vertices for edge in chain.edges()}
    assert edge_vertices == {(0, 1), (1, 2), (2, 3)}


def test_chain1d_vertices():
    """Test that Chain1D vertices are correct."""
    chain = Chain1D(5)
    vertices = list(chain.vertices())
    assert vertices == [0, 1, 2, 3, 4]


def test_chain1d_with_self_loops():
    """Test Chain1D with self-loops enabled."""
    chain = Chain1D(4, self_loops=True)
    assert chain.nvertices == 4
    # 4 self-loops + 3 nearest-neighbor edges = 7
    assert chain.nedges == 7


def test_chain1d_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    chain = Chain1D(3, self_loops=True)
    edge_vertices = {edge.vertices for edge in chain.edges()}
    assert edge_vertices == {(0,), (1,), (2,), (0, 1), (1, 2)}


def test_chain1d_coloring_without_self_loops():
    """Test edge coloring without self-loops."""
    chain = Chain1D(5)
    color = _vertex_color_map(chain)
    # Even edges (0-1, 2-3) should have color 0
    assert color[(0, 1)] == 0
    assert color[(2, 3)] == 0
    # Odd edges (1-2, 3-4) should have color 1
    assert color[(1, 2)] == 1
    assert color[(3, 4)] == 1


def test_chain1d_coloring_with_self_loops():
    """Test edge coloring with self-loops."""
    chain = Chain1D(4, self_loops=True)
    color = _vertex_color_map(chain)
    # Self-loops should have color -1
    assert color[(0,)] == -1
    assert color[(1,)] == -1
    assert color[(2,)] == -1
    assert color[(3,)] == -1
    # Even edges should have color 0, odd edges color 1
    assert color[(0, 1)] == 0
    assert color[(1, 2)] == 1
    assert color[(2, 3)] == 0


def test_chain1d_coloring_non_overlapping():
    """Test that edges with the same color don't share vertices."""
    chain = Chain1D(6)
    coloring = chain.edge_coloring()
    # Group edges by color
    colors = {}
    for edge in chain.edges():
        color = coloring.color(edge.vertices)
        assert color is not None
        edge_vertices = edge.vertices
        if color not in colors:
            colors[color] = []
        colors[color].append(edge_vertices)
    # Check each color group
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_chain1d_str():
    """Test string representation."""
    chain = Chain1D(4)
    assert "4 vertices" in str(chain)
    assert "3 edges" in str(chain)


# MthNearestNeighborChain1D tests


def test_mth_nearest_neighbor_chain1d_init_basic():
    """Test basic m-th nearest-neighbor chain initialization."""
    chain = MthNearestNeighborChain1D(5, 2)
    assert chain.nvertices == 5
    assert chain.nedges == 7
    assert chain.length == 5
    assert chain.m == 2


def test_mth_nearest_neighbor_chain1d_edges_and_marks():
    """Test that edges through distance m are created and marked."""
    chain = MthNearestNeighborChain1D(5, 2)
    edge_marks = {edge.vertices: edge.mark for edge in chain.edges()}
    assert edge_marks == {
        (0, 1): 0,
        (1, 2): 0,
        (2, 3): 0,
        (3, 4): 0,
        (0, 2): 1,
        (1, 3): 1,
        (2, 4): 1,
    }


def test_mth_nearest_neighbor_chain1d_m_one_matches_chain1d():
    """Test that range one has the same edges as a nearest-neighbor chain."""
    chain = MthNearestNeighborChain1D(4, 1)
    edge_vertices = {edge.vertices for edge in chain.edges()}
    assert edge_vertices == {(0, 1), (1, 2), (2, 3)}
    assert all(edge.mark == 0 for edge in chain.edges())


def test_mth_nearest_neighbor_chain1d_with_self_loops():
    """Test that optional self-loops remain unmarked."""
    chain = MthNearestNeighborChain1D(4, 2, self_loops=True)
    self_loops = {
        edge.vertices: edge.mark for edge in chain.edges() if len(edge.vertices) == 1
    }
    assert self_loops == {(0,): None, (1,): None, (2,): None, (3,): None}
    assert chain.nedges == 9


def test_mth_nearest_neighbor_chain1d_m_exceeds_length():
    """Test that the interaction range is limited by the chain length."""
    chain = MthNearestNeighborChain1D(4, 10)
    edge_vertices = {edge.vertices for edge in chain.edges()}
    assert edge_vertices == {
        (0, 1),
        (0, 2),
        (0, 3),
        (1, 2),
        (1, 3),
        (2, 3),
    }


def test_mth_nearest_neighbor_chain1d_coloring():
    """Test that each distance uses two alternating colors."""
    chain = MthNearestNeighborChain1D(7, 3)
    assert isinstance(chain, Hypergraph)
    coloring = chain.edge_coloring()
    assert isinstance(coloring, HypergraphEdgeColoring)
    assert _vertex_color_map(chain) == {
        (0, 1): 0,
        (1, 2): 1,
        (2, 3): 0,
        (3, 4): 1,
        (4, 5): 0,
        (5, 6): 1,
        (0, 2): 2,
        (1, 3): 2,
        (2, 4): 3,
        (3, 5): 3,
        (4, 6): 2,
        (0, 3): 4,
        (1, 4): 4,
        (2, 5): 4,
        (3, 6): 5,
    }


def test_mth_nearest_neighbor_chain1d_coloring_is_valid():
    """Test that same-color range-neighbor edges do not overlap."""
    chain = MthNearestNeighborChain1D(8, 3)
    coloring = chain.edge_coloring()
    for color in coloring.colors():
        used_vertices = set()
        for edge in coloring.edges_of_color(color):
            assert used_vertices.isdisjoint(edge.vertices)
            used_vertices.update(edge.vertices)


def test_mth_nearest_neighbor_chain1d_coloring_with_self_loops():
    """Test that self-loops use the special negative color."""
    chain = MthNearestNeighborChain1D(4, 2, self_loops=True)
    coloring = chain.edge_coloring()
    assert all(
        coloring.color(edge.vertices) == -1
        for edge in chain.edges()
        if len(edge.vertices) == 1
    )


# Ring1D tests


def test_ring1d_init_basic():
    """Test basic Ring1D initialization."""
    ring = Ring1D(4)
    assert ring.nvertices == 4
    assert ring.nedges == 4
    assert ring.length == 4


def test_ring1d_two_vertices():
    """Test Ring1D with two vertices (two edges, same pair)."""
    ring = Ring1D(2)
    assert ring.nvertices == 2
    # Edge 0-1 and edge 1-0 (wrapping), but both are [0,1] after sorting
    assert ring.nedges == 2


def test_ring1d_three_vertices():
    """Test Ring1D with three vertices (triangle)."""
    ring = Ring1D(3)
    assert ring.nvertices == 3
    assert ring.nedges == 3


def test_ring1d_edges():
    """Test that Ring1D creates correct edges including wrap-around."""
    ring = Ring1D(4)
    edge_vertices = {edge.vertices for edge in ring.edges()}
    assert edge_vertices == {(0, 1), (1, 2), (2, 3), (0, 3)}


def test_ring1d_vertices():
    """Test that Ring1D vertices are correct."""
    ring = Ring1D(5)
    vertices = list(ring.vertices())
    assert vertices == [0, 1, 2, 3, 4]


def test_ring1d_with_self_loops():
    """Test Ring1D with self-loops enabled."""
    ring = Ring1D(4, self_loops=True)
    assert ring.nvertices == 4
    # 4 self-loops + 4 nearest-neighbor edges = 8
    assert ring.nedges == 8


def test_ring1d_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    ring = Ring1D(3, self_loops=True)
    edge_vertices = {edge.vertices for edge in ring.edges()}
    assert edge_vertices == {(0,), (1,), (2,), (0, 1), (1, 2), (0, 2)}


def test_ring1d_coloring_without_self_loops():
    """Test edge coloring without self-loops."""
    ring = Ring1D(4)
    color = _vertex_color_map(ring)
    # Even edges should have color 0, odd edges color 1
    assert color[(0, 1)] == 0
    assert color[(1, 2)] == 1
    assert color[(2, 3)] == 0
    assert color[(0, 3)] == 1  # Wrap-around edge


def test_ring1d_coloring_with_self_loops():
    """Test edge coloring with self-loops."""
    ring = Ring1D(4, self_loops=True)
    color = _vertex_color_map(ring)
    # Self-loops should have color -1
    assert color[(0,)] == -1
    assert color[(1,)] == -1
    assert color[(2,)] == -1
    assert color[(3,)] == -1
    # Even edges should have color 0, odd edges color 1
    assert color[(0, 1)] == 0
    assert color[(1, 2)] == 1
    assert color[(2, 3)] == 0
    assert color[(0, 3)] == 1


def test_ring1d_coloring_non_overlapping():
    """Test that edges with the same color don't share vertices."""
    ring = Ring1D(6)
    coloring = ring.edge_coloring()
    # Group edges by color
    colors = {}
    for edge in ring.edges():
        color = coloring.color(edge.vertices)
        assert color is not None
        edge_vertices = edge.vertices
        if color not in colors:
            colors[color] = []
        colors[color].append(edge_vertices)
    # Check each color group
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_ring1d_str():
    """Test string representation."""
    ring = Ring1D(4)
    assert "4 vertices" in str(ring)
    assert "4 edges" in str(ring)


def test_ring1d_vs_chain1d_edge_count():
    """Test that ring has one more edge than chain of same length."""
    for length in range(2, 10):
        chain = Chain1D(length)
        ring = Ring1D(length)
        assert ring.nedges == chain.nedges + 1


def test_chain1d_inherits_hypergraph():
    """Test that Chain1D is a Hypergraph subclass with all methods."""
    chain = Chain1D(4)
    assert isinstance(chain, Hypergraph)
    # Test inherited methods work
    assert hasattr(chain, "edges")
    assert hasattr(chain, "vertices")
    coloring = chain.edge_coloring()
    assert isinstance(coloring, HypergraphEdgeColoring)
    assert hasattr(coloring, "edges_of_color")


def test_ring1d_inherits_hypergraph():
    """Test that Ring1D is a Hypergraph subclass with all methods."""
    ring = Ring1D(4)
    assert isinstance(ring, Hypergraph)
    # Test inherited methods work
    assert hasattr(ring, "edges")
    assert hasattr(ring, "vertices")
    coloring = ring.edge_coloring()
    assert isinstance(coloring, HypergraphEdgeColoring)
    assert hasattr(coloring, "edges_of_color")
