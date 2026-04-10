# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""OpenQASM interoperability for the Q# ecosystem.

This module re-exports all public symbols from ``qsharp.openqasm``,
making them available under the ``qdk.openqasm`` namespace. It provides
functions for importing, compiling, running, and estimating resources for
OpenQASM 2.0 and 3.0 programs using the local Q# toolchain.

Key exports:

- ``import_openqasm`` — parse and import an OpenQASM program into the Q# interpreter.
- ``run`` — execute an OpenQASM program and return shot results.
- ``compile`` — compile an OpenQASM program to QIR.
- ``estimate`` — run the Microsoft Resource Estimator on an OpenQASM program.
- ``circuit`` — synthesize a circuit diagram from an OpenQASM program.
"""

from qsharp.openqasm import *  # pyright: ignore[reportWildcardImportFromLibrary]
