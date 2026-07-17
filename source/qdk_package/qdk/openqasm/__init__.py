# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""OpenQASM tooling for the QDK.

This module provides functions for compiling, running, estimating, and
generating circuits from OpenQASM 2.0/3.0 programs.

Key exports:

- :func:`~qdk.openqasm.import_openqasm` — import an OpenQASM program or
  fragment into the active Q# interpreter context.
- :func:`~qdk.openqasm.run` — simulate an OpenQASM program for one or more shots.
- :func:`~qdk.openqasm.compile` — compile an OpenQASM program to QIR for
  submission to a hardware target.
- :func:`~qdk.openqasm.circuit` — synthesize a circuit diagram from an
  OpenQASM program.
- :func:`~qdk.openqasm.estimate` — estimate the quantum resources required to
  run an OpenQASM program (deprecated; use :mod:`qdk.qre` instead).
- :class:`~qdk.openqasm.ProgramType` — controls how the source is interpreted
  (``Operation``, ``File``, or ``Fragments``).
- :class:`~qdk.openqasm.OutputSemantics` — controls measurement output
  semantics during compilation.
- :class:`~qdk.openqasm.QasmError` — raised when an OpenQASM source cannot
  be parsed or compiled.
- :mod:`~qdk.openqasm.parser` — the syntactic AST: :func:`parse` and the
  read-only ``openqasm3``-style node classes it produces.
- :mod:`~qdk.openqasm.semantic` — the resolved semantic AST: :func:`analyze`
  and the richly-typed, clean-named node classes it produces (for example
  :class:`~qdk.openqasm.semantic.QuantumGate` and
  :class:`~qdk.openqasm.semantic.BinaryExpression`).
- :class:`~qdk.openqasm.parser.QASMVisitor` — a read-only visitor base for
  walking either the syntactic or semantic AST.
"""

from . import parser, semantic, source
from .parser import (
  CANONICAL_FORMAT_VERSION,
  QASMUnparseError,
  dump,
  dumps,
  unparse,
)
from .source import Position, PositionEncoding, SourceEdit, SourceRange
from ._circuit import circuit
from ._compile import compile
from ._estimate import estimate
from ._import import import_openqasm
from ._run import run
from .._native import ProgramType, OutputSemantics, QasmError  # type: ignore

__all__ = [
    "circuit",
    "compile",
    "estimate",
    "import_openqasm",
    "run",
    "dumps",
    "unparse",
    "dump",
    "CANONICAL_FORMAT_VERSION",
    "QASMUnparseError",
    "parser",
    "semantic",
    "source",
    "Position",
    "PositionEncoding",
    "SourceEdit",
    "SourceRange",
    "ProgramType",
    "OutputSemantics",
    "QasmError",
]
