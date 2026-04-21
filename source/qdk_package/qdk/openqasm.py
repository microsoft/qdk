# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""OpenQASM interoperability for the Q# ecosystem.

This module re-exports all public symbols from [qsharp.openqasm](:mod:`qsharp.openqasm`),
making them available under the ``qdk.openqasm`` namespace. It provides
functions for importing, compiling, running, and estimating resources for
OpenQASM 2.0 and 3.0 programs using the local Q# toolchain.

Key exports:

- :func:`~qsharp.openqasm.import_openqasm` — parse and import an OpenQASM program into the Q# interpreter.
- :func:`~qsharp.openqasm.run` — execute an OpenQASM program and return shot results.
- :func:`~qsharp.openqasm.estimate` — run the Microsoft Resource Estimator on an OpenQASM program.
- :func:`~qsharp.openqasm.circuit` — synthesize a circuit diagram from an OpenQASM program.

For full API documentation see [qsharp.openqasm](:mod:`qsharp.openqasm`).
"""

from qsharp.openqasm import *  # pyright: ignore[reportWildcardImportFromLibrary]
