# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Utilities module for magnets package.

This module provides utility data structures and algorithms used across
the magnets package, including hypergraph representations.
"""

from .fermion import (
    Fermion,
    FermionAnnihilation,
    FermionCreation,
    FermionString,
    hopping_term,
)
from .hypergraph import (
    Hyperedge,
    Hypergraph,
    HypergraphEdgeColoring,
)
from .majorana import (
    edge_operator,
    Majorana,
    MajoranaDualFermion,
    MajoranaFermion,
    MajoranaString,
    vertex_operator,
)
from .pauli import (
    Pauli,
    PauliString,
    PauliX,
    PauliY,
    PauliZ,
)

__all__ = [
    "Hyperedge",
    "Hypergraph",
    "HypergraphEdgeColoring",
    "Fermion",
    "FermionAnnihilation",
    "FermionCreation",
    "FermionString",
    "hopping_term",
    "Majorana",
    "MajoranaDualFermion",
    "MajoranaFermion",
    "MajoranaString",
    "edge_operator",
    "vertex_operator",
    "Pauli",
    "PauliString",
    "PauliX",
    "PauliY",
    "PauliZ",
]
