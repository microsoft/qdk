# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Models module for quantum spin models.

This module provides classes for representing quantum spin models
as Hamiltonians built from Pauli operators.
"""

from .model import IsingModel, Model

__all__ = ["Model", "IsingModel"]
