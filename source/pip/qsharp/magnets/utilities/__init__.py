# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Utilities module for magnets package.

This module provides utility data structures and algorithms used across
the magnets package, including hypergraph representations.
"""

from .hypergraph import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
    edge_coloring,
    greedy_edge_coloring,
)
from .pauli import Pauli, PauliString, PauliX, PauliY, PauliZ

__all__ = [
    "Hyperedge",
    "Hypergraph",
    "HypergraphEdgeColoring",
    "Pauli",
    "PauliString",
    "PauliX",
    "PauliY",
    "PauliZ",
    "greedy_edge_coloring",
    "edge_coloring",
]
