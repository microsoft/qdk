"""Geometry module for representing quantum system topologies.

This module provides hypergraph data structures for representing the
geometric structure of quantum systems, including lattice topologies
and interaction graphs.
"""

from .hypergraph import Hyperedge, Hypergraph

__all__ = ["Hyperedge", "Hypergraph"]
