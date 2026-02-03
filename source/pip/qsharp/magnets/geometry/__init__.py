# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Geometry module for representing quantum system topologies.

This module provides hypergraph data structures for representing the
geometric structure of quantum systems, including lattice topologies
and interaction graphs.
"""

from .hypergraph import Hyperedge, Hypergraph, greedy_edge_coloring
from .lattice1d import Chain1D, Ring1D

__all__ = [
    "Hyperedge",
    "Hypergraph",
    "greedy_edge_coloring",
    "Chain1D",
    "Ring1D",
]
