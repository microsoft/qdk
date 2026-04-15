# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Resource estimation utilities for the Q# ecosystem.

This module re-exports all public symbols from ``qsharp.estimator``,
making them available under the ``qdk.estimator`` namespace. It provides
classes for configuring and interpreting Microsoft Resource Estimator jobs,
including qubit parameter models, QEC schemes, distillation unit
specifications, error budgets, and the result container.

Key exports:

- ``EstimatorParams`` — top-level input parameters for a resource estimation job.
- ``EstimatorResult`` — result container with formatted tables and diagrams.
- ``LogicalCounts`` — pre-calculated logical resource counts for physical estimation.
- ``QubitParams``, ``QECScheme`` — predefined model name constants.
- ``EstimatorQubitParams``, ``EstimatorQecScheme`` — custom model configuration.
- ``ErrorBudgetPartition``, ``EstimatorConstraints`` — budget and constraint settings.
"""

from qsharp.estimator import *  # pyright: ignore[reportWildcardImportFromLibrary]
