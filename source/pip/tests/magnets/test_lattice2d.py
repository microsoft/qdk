# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for 2D lattice data structures."""

from qsharp.magnets.geometry.lattice2d import Patch2D, Torus2D


# Patch2D tests


def test_patch2d_init_basic():
    """Test basic Patch2D initialization."""
    patch = Patch2D(3, 2)
    assert patch.nvertices == 6
    # 2 horizontal edges per row * 2 rows + 3 vertical edges per column * 1 = 7
    assert patch.nedges == 7
    assert patch.width == 3
    assert patch.height == 2


def test_patch2d_single_vertex():
    """Test Patch2D with a single vertex (no edges)."""
    patch = Patch2D(1, 1)
    assert patch.nvertices == 0
    assert patch.nedges == 0
    assert patch.width == 1
    assert patch.height == 1


def test_patch2d_single_row():
    """Test Patch2D with a single row (like Chain1D)."""
    patch = Patch2D(4, 1)
    assert patch.nvertices == 4
    assert patch.nedges == 3  # Only horizontal edges


def test_patch2d_single_column():
    """Test Patch2D with a single column."""
    patch = Patch2D(1, 4)
    assert patch.nvertices == 4
    assert patch.nedges == 3  # Only vertical edges


def test_patch2d_square():
    """Test Patch2D with a square lattice."""
    patch = Patch2D(3, 3)
    assert patch.nvertices == 9
    # 2 horizontal * 3 rows + 3 vertical * 2 = 12
    assert patch.nedges == 12


def test_patch2d_edges():
    """Test that Patch2D creates correct nearest-neighbor edges."""
    patch = Patch2D(2, 2)
    edges = list(patch.edges())
    # Should have 4 edges: 2 horizontal + 2 vertical
    assert len(edges) == 4
    # Vertices: 0=(0,0), 1=(1,0), 2=(0,1), 3=(1,1)
    # Horizontal: [0,1], [2,3]
    # Vertical: [0,2], [1,3]
    edge_sets = [set(e.vertices) for e in edges]
    assert {0, 1} in edge_sets
    assert {2, 3} in edge_sets
    assert {0, 2} in edge_sets
    assert {1, 3} in edge_sets


def test_patch2d_vertices():
    """Test that Patch2D vertices are correct."""
    patch = Patch2D(3, 2)
    vertices = list(patch.vertices())
    assert vertices == [0, 1, 2, 3, 4, 5]


def test_patch2d_with_self_loops():
    """Test Patch2D with self-loops enabled."""
    patch = Patch2D(3, 2, self_loops=True)
    assert patch.nvertices == 6
    # 6 self-loops + 7 nearest-neighbor edges = 13
    assert patch.nedges == 13


def test_patch2d_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    patch = Patch2D(2, 2, self_loops=True)
    edges = list(patch.edges())
    # First 4 edges should be self-loops
    assert edges[0].vertices == (0,)
    assert edges[1].vertices == (1,)
    assert edges[2].vertices == (2,)
    assert edges[3].vertices == (3,)


def test_patch2d_coloring_without_self_loops():
    """Test edge coloring without self-loops."""
    patch = Patch2D(4, 4)
    # Should have 4 colors: horizontal even/odd (0,1), vertical even/odd (2,3)
    assert patch.ncolors == 4


def test_patch2d_coloring_with_self_loops():
    """Test edge coloring with self-loops."""
    patch = Patch2D(3, 3, self_loops=True)
    # Should have 5 colors: self-loops (-1) + 4 edge groups (0-3)
    assert patch.ncolors == 5


def test_patch2d_coloring_non_overlapping():
    """Test that edges with the same color don't share vertices."""
    patch = Patch2D(4, 4)
    # Group edges by color
    colors = {}
    for edge_vertices, color in patch.color.items():
        if color not in colors:
            colors[color] = []
        colors[color].append(edge_vertices)
    # Check each color group
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_patch2d_str():
    """Test string representation."""
    patch = Patch2D(3, 2)
    assert "6 vertices" in str(patch)
    assert "7 edges" in str(patch)


# Torus2D tests


def test_torus2d_init_basic():
    """Test basic Torus2D initialization."""
    torus = Torus2D(3, 2)
    assert torus.nvertices == 6
    # 3 horizontal edges per row * 2 rows + 3 vertical edges per column * 2 = 12
    assert torus.nedges == 12
    assert torus.width == 3
    assert torus.height == 2


def test_torus2d_single_vertex():
    """Test Torus2D with a single vertex (self-edge in both directions)."""
    torus = Torus2D(1, 1)
    assert torus.nvertices == 1
    # One horizontal wrap + one vertical wrap, both connect vertex 0 to itself
    assert torus.nedges == 2


def test_torus2d_single_row():
    """Test Torus2D with a single row (like Ring1D + vertical wraps)."""
    torus = Torus2D(4, 1)
    assert torus.nvertices == 4
    # 4 horizontal + 4 vertical wraps
    assert torus.nedges == 8


def test_torus2d_single_column():
    """Test Torus2D with a single column."""
    torus = Torus2D(1, 4)
    assert torus.nvertices == 4
    # 4 horizontal wraps + 4 vertical
    assert torus.nedges == 8


def test_torus2d_square():
    """Test Torus2D with a square lattice."""
    torus = Torus2D(3, 3)
    assert torus.nvertices == 9
    # 3 horizontal * 3 rows + 3 vertical * 3 columns = 18
    assert torus.nedges == 18


def test_torus2d_edges():
    """Test that Torus2D creates correct edges including wrap-around."""
    torus = Torus2D(2, 2)
    edges = list(torus.edges())
    # Should have 8 edges: 4 horizontal + 4 vertical
    assert len(edges) == 8
    # Vertices: 0=(0,0), 1=(1,0), 2=(0,1), 3=(1,1)
    edge_sets = [set(e.vertices) for e in edges]
    # Horizontal edges (including wraps)
    assert {0, 1} in edge_sets  # (0,0)-(1,0)
    assert {2, 3} in edge_sets  # (0,1)-(1,1)
    # Vertical edges (including wraps)
    assert {0, 2} in edge_sets  # (0,0)-(0,1)
    assert {1, 3} in edge_sets  # (1,0)-(1,1)


def test_torus2d_vertices():
    """Test that Torus2D vertices are correct."""
    torus = Torus2D(3, 2)
    vertices = list(torus.vertices())
    assert vertices == [0, 1, 2, 3, 4, 5]


def test_torus2d_with_self_loops():
    """Test Torus2D with self-loops enabled."""
    torus = Torus2D(3, 2, self_loops=True)
    assert torus.nvertices == 6
    # 6 self-loops + 12 nearest-neighbor edges = 18
    assert torus.nedges == 18


def test_torus2d_self_loops_edges():
    """Test that self-loop edges are created correctly."""
    torus = Torus2D(2, 2, self_loops=True)
    edges = list(torus.edges())
    # First 4 edges should be self-loops
    assert edges[0].vertices == (0,)
    assert edges[1].vertices == (1,)
    assert edges[2].vertices == (2,)
    assert edges[3].vertices == (3,)


def test_torus2d_coloring_without_self_loops():
    """Test edge coloring without self-loops."""
    torus = Torus2D(4, 4)
    # Should have 4 colors: horizontal even/odd (0,1), vertical even/odd (2,3)
    assert torus.ncolors == 4


def test_torus2d_coloring_with_self_loops():
    """Test edge coloring with self-loops."""
    torus = Torus2D(3, 3, self_loops=True)
    # Should have 5 colors: self-loops (-1) + 4 edge groups (0-3)
    assert torus.ncolors == 5


def test_torus2d_coloring_non_overlapping():
    """Test that edges with the same color don't share vertices."""
    torus = Torus2D(4, 4)
    # Group edges by color
    colors = {}
    for edge_vertices, color in torus.color.items():
        if color not in colors:
            colors[color] = []
        colors[color].append(edge_vertices)
    # Check each color group
    for color, edge_list in colors.items():
        used_vertices = set()
        for vertices in edge_list:
            assert not any(v in used_vertices for v in vertices)
            used_vertices.update(vertices)


def test_torus2d_str():
    """Test string representation."""
    torus = Torus2D(3, 2)
    assert "6 vertices" in str(torus)
    assert "12 edges" in str(torus)


def test_torus2d_vs_patch2d_edge_count():
    """Test that torus has more edges than patch of same dimensions."""
    for width in range(2, 5):
        for height in range(2, 5):
            patch = Patch2D(width, height)
            torus = Torus2D(width, height)
            # Torus has width + height extra edges (wrapping)
            assert torus.nedges == patch.nedges + width + height


def test_patch2d_inherits_hypergraph():
    """Test that Patch2D is a Hypergraph subclass with all methods."""
    from qsharp.magnets.utilities import Hypergraph

    patch = Patch2D(3, 3)
    assert isinstance(patch, Hypergraph)
    # Test inherited methods work
    assert hasattr(patch, "edges")
    assert hasattr(patch, "vertices")
    assert hasattr(patch, "edges_by_color")


def test_torus2d_inherits_hypergraph():
    """Test that Torus2D is a Hypergraph subclass with all methods."""
    from qsharp.magnets.utilities import Hypergraph

    torus = Torus2D(3, 3)
    assert isinstance(torus, Hypergraph)
    # Test inherited methods work
    assert hasattr(torus, "edges")
    assert hasattr(torus, "vertices")
    assert hasattr(torus, "edges_by_color")
