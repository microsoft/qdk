"""Unit tests for hypergraph data structures."""

import unittest
from qsharp.magnets.geometry.hypergraph import Hyperedge, Hypergraph


class TestHyperedge(unittest.TestCase):
    """Test cases for the Hyperedge class."""

    def test_init_basic(self):
        """Test basic Hyperedge initialization."""
        edge = Hyperedge([0, 1])
        self.assertEqual(edge.vertices, [0, 1])

    def test_vertices_sorted(self):
        """Test that vertices are automatically sorted."""
        edge = Hyperedge([3, 1, 2])
        self.assertEqual(edge.vertices, [1, 2, 3])

    def test_single_vertex(self):
        """Test hyperedge with single vertex (self-loop)."""
        edge = Hyperedge([5])
        self.assertEqual(edge.vertices, [5])
        self.assertEqual(len(edge.vertices), 1)

    def test_multiple_vertices(self):
        """Test hyperedge with multiple vertices (multi-body interaction)."""
        edge = Hyperedge([0, 1, 2, 3])
        self.assertEqual(edge.vertices, [0, 1, 2, 3])
        self.assertEqual(len(edge.vertices), 4)

    def test_repr(self):
        """Test string representation."""
        edge = Hyperedge([1, 0])
        self.assertEqual(repr(edge), "Hyperedge([0, 1])")

    def test_empty_vertices(self):
        """Test hyperedge with empty vertex list."""
        edge = Hyperedge([])
        self.assertEqual(edge.vertices, [])
        self.assertEqual(len(edge.vertices), 0)


class TestHypergraph(unittest.TestCase):
    """Test cases for the Hypergraph class."""

    def test_init_basic(self):
        """Test basic Hypergraph initialization."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nedges(), 2)
        self.assertEqual(graph.nvertices(), 3)

    def test_empty_graph(self):
        """Test hypergraph with no edges."""
        graph = Hypergraph([])
        self.assertEqual(graph.nedges(), 0)
        self.assertEqual(graph.nvertices(), 0)

    def test_nedges(self):
        """Test edge count."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nedges(), 3)

    def test_nvertices(self):
        """Test vertex count with unique vertices."""
        edges = [Hyperedge([0, 1]), Hyperedge([2, 3])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nvertices(), 4)

    def test_nvertices_with_shared_vertices(self):
        """Test vertex count when edges share vertices."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([0, 2])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nvertices(), 3)

    def test_vertices_iterator(self):
        """Test vertices iterator returns sorted vertices."""
        edges = [Hyperedge([3, 1]), Hyperedge([0, 2])]
        graph = Hypergraph(edges)
        vertices = list(graph.vertices())
        self.assertEqual(vertices, [0, 1, 2, 3])

    def test_edges_iterator(self):
        """Test edges iterator returns all edges."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
        graph = Hypergraph(edges)
        edge_list = list(graph.edges())
        self.assertEqual(len(edge_list), 2)

    def test_edges_with_part_parameter(self):
        """Test edges iterator with part parameter (base class ignores it)."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2])]
        graph = Hypergraph(edges)
        # Base class returns all edges regardless of part parameter
        edge_list_0 = list(graph.edges(part=0))
        edge_list_1 = list(graph.edges(part=1))
        self.assertEqual(len(edge_list_0), 2)
        self.assertEqual(len(edge_list_1), 2)

    def test_str(self):
        """Test string representation."""
        edges = [Hyperedge([0, 1]), Hyperedge([1, 2]), Hyperedge([2, 3])]
        graph = Hypergraph(edges)
        expected = "Hypergraph with 4 vertices and 3 edges."
        self.assertEqual(str(graph), expected)

    def test_repr(self):
        """Test repr representation."""
        edges = [Hyperedge([0, 1])]
        graph = Hypergraph(edges)
        result = repr(graph)
        self.assertIn("Hypergraph", result)
        self.assertIn("Hyperedge", result)

    def test_single_vertex_edges(self):
        """Test hypergraph with self-loop edges."""
        edges = [Hyperedge([0]), Hyperedge([1]), Hyperedge([2])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nedges(), 3)
        self.assertEqual(graph.nvertices(), 3)

    def test_mixed_edge_sizes(self):
        """Test hypergraph with edges of different sizes."""
        edges = [
            Hyperedge([0]),  # 1 vertex (self-loop)
            Hyperedge([1, 2]),  # 2 vertices (pair)
            Hyperedge([3, 4, 5]),  # 3 vertices (triple)
        ]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nedges(), 3)
        self.assertEqual(graph.nvertices(), 6)

    def test_non_contiguous_vertices(self):
        """Test hypergraph with non-contiguous vertex indices."""
        edges = [Hyperedge([0, 10]), Hyperedge([5, 20])]
        graph = Hypergraph(edges)
        self.assertEqual(graph.nvertices(), 4)
        vertices = list(graph.vertices())
        self.assertEqual(vertices, [0, 5, 10, 20])


if __name__ == "__main__":
    unittest.main()
