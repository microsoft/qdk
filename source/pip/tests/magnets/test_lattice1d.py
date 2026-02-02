# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for 1D lattice data structures."""

from qsharp.magnets.geometry.lattice1d import Chain1D, Ring1D


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
    edges = list(chain.edges())
    assert len(edges) == 3
    # Check edges are [0,1], [1,2], [2,3]
    assert edges[0].vertices == [0, 1]
    assert edges[1].vertices == [1, 2]
    assert edges[2].vertices == [2, 3]


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
    edges = list(chain.edges())
    # First 3 edges should be self-loops
    assert edges[0].vertices == [0]
    assert edges[1].vertices == [1]
    assert edges[2].vertices == [2]
    # Next 2 edges should be nearest-neighbor
    assert edges[3].vertices == [0, 1]
    assert edges[4].vertices == [1, 2]


def test_chain1d_parts_without_self_loops():
    """Test edge partitioning without self-loops."""
    chain = Chain1D(5)
    # Should have 2 parts: even edges [0,2] and odd edges [1,3]
    assert len(chain.parts) == 2
    assert chain.parts[0] == [0, 2]  # edges 0-1, 2-3
    assert chain.parts[1] == [1, 3]  # edges 1-2, 3-4


def test_chain1d_parts_with_self_loops():
    """Test edge partitioning with self-loops."""
    chain = Chain1D(4, self_loops=True)
    # Should have 3 parts: self-loops, even edges, odd edges
    assert len(chain.parts) == 3


def test_chain1d_parts_non_overlapping():
    """Test that edges in the same part don't share vertices."""
    chain = Chain1D(6)
    for part_indices in chain.parts:
        used_vertices = set()
        for idx in part_indices:
            edge = chain._edge_list[idx]
            assert not any(v in used_vertices for v in edge.vertices)
            used_vertices.update(edge.vertices)


def test_chain1d_str():
    """Test string representation."""
    chain = Chain1D(4)
    assert "4 vertices" in str(chain)
    assert "3 edges" in str(chain)


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
    edges = list(ring.edges())
    assert len(edges) == 4
    # Check edges are [0,1], [1,2], [2,3], [0,3] (sorted)
    assert edges[0].vertices == [0, 1]
    assert edges[1].vertices == [1, 2]
    assert edges[2].vertices == [2, 3]
    assert edges[3].vertices == [0, 3]  # Wrap-around edge


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
    edges = list(ring.edges())
    # First 3 edges should be self-loops
    assert edges[0].vertices == [0]
    assert edges[1].vertices == [1]
    assert edges[2].vertices == [2]
    # Next 3 edges should be nearest-neighbor (including wrap)
    assert edges[3].vertices == [0, 1]
    assert edges[4].vertices == [1, 2]
    assert edges[5].vertices == [0, 2]  # Wrap-around


def test_ring1d_parts_without_self_loops():
    """Test edge partitioning without self-loops."""
    ring = Ring1D(4)
    # Should have 2 parts for parallel updates
    assert len(ring.parts) == 2


def test_ring1d_parts_with_self_loops():
    """Test edge partitioning with self-loops."""
    ring = Ring1D(4, self_loops=True)
    # Should have 3 parts: self-loops, even edges, odd edges
    assert len(ring.parts) == 3


def test_ring1d_parts_non_overlapping():
    """Test that edges in the same part don't share vertices."""
    ring = Ring1D(6)
    for part_indices in ring.parts:
        used_vertices = set()
        for idx in part_indices:
            edge = ring._edge_list[idx]
            assert not any(v in used_vertices for v in edge.vertices)
            used_vertices.update(edge.vertices)


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
    from qsharp.magnets.geometry.hypergraph import Hypergraph

    chain = Chain1D(4)
    assert isinstance(chain, Hypergraph)
    # Test inherited methods work
    assert hasattr(chain, "edges")
    assert hasattr(chain, "vertices")
    assert hasattr(chain, "edgesByPart")


def test_ring1d_inherits_hypergraph():
    """Test that Ring1D is a Hypergraph subclass with all methods."""
    from qsharp.magnets.geometry.hypergraph import Hypergraph

    ring = Ring1D(4)
    assert isinstance(ring, Hypergraph)
    # Test inherited methods work
    assert hasattr(ring, "edges")
    assert hasattr(ring, "vertices")
    assert hasattr(ring, "edgesByPart")
