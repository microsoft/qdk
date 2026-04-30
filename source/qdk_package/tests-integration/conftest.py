# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""
This file is used to configure pytest for the test suite.

- It attempts to import necessary modules from test_circuits.
- It monkey-patches extra symbols onto the ``qdk`` module so that
  ``import qdk as qsharp`` followed by ``qsharp.compile(...)`` etc. works.

Fixtures and other configurations for pytest can be added to this file to
be shared across multiple test files.
"""

# ---------------------------------------------------------------------------
# Monkey-patch symbols onto qdk that are NOT part of the public API but are
# used throughout tests via the ``import qdk as qsharp`` alias.
# ---------------------------------------------------------------------------
import qdk
from qdk._qsharp import (  # noqa: E402
    eval,
    run,
    compile,
    circuit,
    estimate,
    logical_counts,
    QSharpError,
    CircuitGenerationMethod,
)
from qdk._native import estimate_custom  # type: ignore  # noqa: E402

qdk.eval = eval
qdk.run = run
qdk.compile = compile
qdk.circuit = circuit
qdk.estimate = estimate
qdk.logical_counts = logical_counts
qdk.QSharpError = QSharpError
qdk.CircuitGenerationMethod = CircuitGenerationMethod
qdk.estimate_custom = estimate_custom

from interop_qiskit.test_circuits import *
