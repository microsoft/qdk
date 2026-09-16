# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Geometry module for representing quantum system topologies.

This module provides hypergraph data structures for representing the
geometric structure of quantum systems, including lattice topologies
and interaction graphs.
"""

from .complete import CompleteBipartiteGraph, CompleteGraph
from .lattice1d import Chain1D, MthNearestNeighborChain1D, Ring1D
from .lattice2d import Patch2D, SecondNearestNeighborPatch2D, Torus2D

__all__ = [
    "CompleteBipartiteGraph",
    "CompleteGraph",
    "Chain1D",
    "MthNearestNeighborChain1D",
    "Ring1D",
    "Patch2D",
    "SecondNearestNeighborPatch2D",
    "Torus2D",
]
