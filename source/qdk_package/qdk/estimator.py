# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Resource estimation utilities for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.estimator](:mod:`qsharp.estimator`),
making them available under the ``qdk.estimator`` namespace. It provides
classes for configuring and interpreting Microsoft Resource Estimator jobs.

Key exports:

- :class:`~qsharp.estimator.EstimatorParams` — top-level input parameters for a resource estimation job.
- :class:`~qsharp.estimator.EstimatorResult` — result container with formatted tables and diagrams.
- :class:`~qsharp.estimator.LogicalCounts` — pre-calculated logical resource counts for physical estimation.
- :class:`~qsharp.estimator.QubitParams`, :class:`~qsharp.estimator.QECScheme` — predefined model name constants.
- :class:`~qsharp.estimator.EstimatorQubitParams`, :class:`~qsharp.estimator.EstimatorQecScheme` — custom model configuration.
- :class:`~qsharp.estimator.ErrorBudgetPartition` — budget and constraint settings.

For full API documentation see [qsharp.estimator](:mod:`qsharp.estimator`).
"""

from qsharp.estimator import *  # pyright: ignore[reportWildcardImportFromLibrary]
